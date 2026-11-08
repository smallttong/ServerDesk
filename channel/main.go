// Command serverdesk-channel runs a userspace Tailscale node (tsnet) and
// exposes a local SOCKS5 endpoint, so the desktop shell can reach hosts on the
// tailnet without installing a system-wide VPN, without a TUN adapter and
// without administrator rights.
//
// Protocol: one JSON object per line on stdout.
//
//	{"event":"listening","port":17890}
//	{"event":"login","url":"https://login.tailscale.com/a/..."}
//	{"event":"status","backend":"Running","ip":"100.x.y.z"}
//	{"event":"error","message":"..."}
//
// The process exits when stdin is closed, which happens automatically when the
// desktop app exits.
package main

import (
	"bufio"
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"tailscale.com/tsnet"
)

var (
	emitMu         sync.Mutex
	lastAuth       string
	connected      atomic.Bool
	allowedTargets atomic.Value // map[string]struct{}; empty means deny all
)

func targetKey(target string) (string, error) {
	host, portText, err := net.SplitHostPort(target)
	if err != nil {
		return "", err
	}
	host = strings.TrimSuffix(strings.ToLower(strings.TrimSpace(host)), ".")
	if ip := net.ParseIP(host); ip != nil {
		host = ip.String()
	}
	port, err := strconv.Atoi(portText)
	if err != nil || port < 1 || port > 65535 || host == "" || strings.Contains(host, "%") {
		return "", errors.New("invalid target")
	}
	return net.JoinHostPort(host, strconv.Itoa(port)), nil
}

func parseAllowedTargets(raw []string) (map[string]struct{}, error) {
	result := make(map[string]struct{}, len(raw))
	for _, target := range raw {
		key, err := targetKey(target)
		if err != nil {
			return nil, err
		}
		result[key] = struct{}{}
	}
	return result, nil
}

func applyAllowlistUpdate(line []byte) bool {
	var update struct {
		AllowedTargets []string `json:"allowedTargets"`
	}
	if json.Unmarshal(line, &update) != nil || update.AllowedTargets == nil {
		return false
	}
	list, err := parseAllowedTargets(update.AllowedTargets)
	if err != nil {
		return false
	}
	allowedTargets.Store(list)
	return true
}

func emit(payload map[string]any) {
	emitMu.Lock()
	defer emitMu.Unlock()
	data, err := json.Marshal(payload)
	if err != nil {
		return
	}
	_, _ = os.Stdout.Write(append(data, '\n'))
}

func emitError(format string, args ...any) {
	emit(map[string]any{"event": "error", "message": fmt.Sprintf(format, args...)})
}

func main() {
	stateDir := flag.String("state-dir", "", "directory that stores the tailnet node identity")
	hostname := flag.String("hostname", "serverdesk", "node name shown in the tailnet")
	authKey := flag.String("authkey", "", "optional auth key (one-time registration)")
	socksAddr := flag.String("socks-addr", "127.0.0.1:0", "SOCKS5 listen address")
	tags := flag.String("tags", "", "comma separated advertise tags, e.g. tag:serverdesk")
	allowTargets := flag.String("allow-targets", "[]", "JSON array of permitted SOCKS host:port targets")
	flag.Parse()
	var initialTargets []string
	if err := json.Unmarshal([]byte(*allowTargets), &initialTargets); err != nil || initialTargets == nil {
		emitError("无效的 SOCKS 目标白名单")
		os.Exit(1)
	}
	initialAllowlist, err := parseAllowedTargets(initialTargets)
	if err != nil {
		emitError("无效的 SOCKS 目标白名单: %v", err)
		os.Exit(1)
	}
	allowedTargets.Store(initialAllowlist)

	if *stateDir == "" {
		base, err := os.UserConfigDir()
		if err != nil {
			emitError("无法定位配置目录: %v", err)
			os.Exit(1)
		}
		*stateDir = filepath.Join(base, "serverdesk-channel")
	}
	if err := os.MkdirAll(*stateDir, 0o700); err != nil {
		emitError("无法创建状态目录: %v", err)
		os.Exit(1)
	}

	srv := &tsnet.Server{
		Dir:      *stateDir,
		Hostname: *hostname,
		AuthKey:  *authKey,
		Logf: func(format string, args ...any) {
			// tsnet is chatty; keep it in the log stream for troubleshooting
			// but never let it crash the process.
			msg := fmt.Sprintf(format, args...)
			if strings.Contains(msg, "error") || strings.Contains(msg, "magicsock") {
				emit(map[string]any{"event": "log", "message": msg})
			}
		},
	}
	if *tags != "" {
		for _, tag := range strings.Split(*tags, ",") {
			tag = strings.TrimSpace(tag)
			if tag != "" {
				srv.AdvertiseTags = append(srv.AdvertiseTags, tag)
			}
		}
	}
	defer srv.Close()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Only the parent owns stdin. It can atomically replace the allowlist; EOF
	// still ends the sidecar when the desktop app exits.
	go func() {
		scanner := bufio.NewScanner(os.Stdin)
		scanner.Buffer(make([]byte, 4096), 1024*1024)
		for scanner.Scan() {
			applyAllowlistUpdate(scanner.Bytes())
		}
		cancel()
		_ = srv.Close()
		os.Exit(0)
	}()

	// Bind the SOCKS listener first: the port must be known even before the
	// tailnet is authenticated, because every service webview is configured
	// with this proxy at creation time.
	listener, err := net.Listen("tcp", *socksAddr)
	if err != nil {
		emitError("SOCKS5 端口监听失败: %v", err)
		os.Exit(1)
	}
	port := listener.Addr().(*net.TCPAddr).Port
	emit(map[string]any{"event": "listening", "port": port})

	go func() {
		for {
			conn, err := listener.Accept()
			if err != nil {
				if ctx.Err() != nil {
					return
				}
				time.Sleep(50 * time.Millisecond)
				continue
			}
			go handleSOCKS(ctx, srv, conn)
		}
	}()

	go func() {
		if _, err := srv.Up(ctx); err != nil && ctx.Err() == nil {
			emitError("通道启动失败: %v", err)
		}
	}()

	pollStatus(ctx, srv)
}

// pollStatus streams login URLs and connection state to the parent process.
func pollStatus(ctx context.Context, srv *tsnet.Server) {
	lc, err := srv.LocalClient()
	if err != nil {
		emitError("无法连接本地控制接口: %v", err)
		return
	}

	lastState := ""
	for {
		select {
		case <-ctx.Done():
			return
		case <-time.After(time.Second):
		}

		st, err := lc.Status(ctx)
		if err != nil || st == nil {
			continue
		}

		if st.AuthURL != "" && st.AuthURL != lastAuth {
			lastAuth = st.AuthURL
			emit(map[string]any{"event": "login", "url": st.AuthURL})
		}

		ip := ""
		if len(st.TailscaleIPs) > 0 {
			ip = st.TailscaleIPs[0].String()
		}
		state := string(st.BackendState)
		connected.Store(state == "Running")
		key := state + "|" + ip
		if key != lastState {
			lastState = key
			emit(map[string]any{"event": "status", "backend": state, "ip": ip})
		}
	}
}

// handleSOCKS implements the minimal SOCKS5 CONNECT flow the WebView needs.
func handleSOCKS(ctx context.Context, srv *tsnet.Server, conn net.Conn) {
	defer conn.Close()
	_ = conn.SetDeadline(time.Now().Add(30 * time.Second))

	reader := bufio.NewReader(conn)

	// Method negotiation.
	head := make([]byte, 2)
	if _, err := io.ReadFull(reader, head); err != nil {
		return
	}
	if head[0] != 5 {
		return
	}
	methods := make([]byte, int(head[1]))
	if _, err := io.ReadFull(reader, methods); err != nil {
		return
	}
	noAuth := false
	for _, m := range methods {
		if m == 0 {
			noAuth = true
			break
		}
	}
	if !noAuth {
		_, _ = conn.Write([]byte{5, 0xff})
		return
	}
	if _, err := conn.Write([]byte{5, 0}); err != nil {
		return
	}

	// Request.
	req := make([]byte, 4)
	if _, err := io.ReadFull(reader, req); err != nil {
		return
	}
	if req[0] != 5 {
		return
	}
	if req[1] != 1 {
		_, _ = conn.Write([]byte{5, 7, 0, 1, 0, 0, 0, 0, 0, 0})
		return
	}

	var host string
	switch req[3] {
	case 1:
		buf := make([]byte, 4)
		if _, err := io.ReadFull(reader, buf); err != nil {
			return
		}
		host = net.IP(buf).String()
	case 3:
		lenBuf := make([]byte, 1)
		if _, err := io.ReadFull(reader, lenBuf); err != nil {
			return
		}
		name := make([]byte, int(lenBuf[0]))
		if _, err := io.ReadFull(reader, name); err != nil {
			return
		}
		host = string(name)
	case 4:
		buf := make([]byte, 16)
		if _, err := io.ReadFull(reader, buf); err != nil {
			return
		}
		host = net.IP(buf).String()
	default:
		_, _ = conn.Write([]byte{5, 8, 0, 1, 0, 0, 0, 0, 0, 0})
		return
	}

	portBuf := make([]byte, 2)
	if _, err := io.ReadFull(reader, portBuf); err != nil {
		return
	}
	target := net.JoinHostPort(host, strconv.Itoa(int(binary.BigEndian.Uint16(portBuf))))
	key, err := targetKey(target)
	if err != nil {
		_, _ = conn.Write([]byte{5, 2, 0, 1, 0, 0, 0, 0, 0, 0})
		return
	}
	if _, ok := allowedTargets.Load().(map[string]struct{})[key]; !ok {
		_, _ = conn.Write([]byte{5, 2, 0, 1, 0, 0, 0, 0, 0, 0})
		return
	}

	// Fail fast while the node is not authenticated, instead of letting every
	// request sit in tsnet's dial until the timeout.
	if !connected.Load() {
		emit(map[string]any{
			"event":   "dial-error",
			"target":  target,
			"message": "通道未就绪（尚未完成 Tailscale 授权）",
		})
		_, _ = conn.Write([]byte{5, 4, 0, 1, 0, 0, 0, 0, 0, 0})
		return
	}

	dialCtx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	upstream, err := srv.Dial(dialCtx, "tcp", target)
	if err != nil {
		emit(map[string]any{"event": "dial-error", "target": target, "message": err.Error()})
		_, _ = conn.Write([]byte{5, 5, 0, 1, 0, 0, 0, 0, 0, 0})
		return
	}
	defer upstream.Close()

	if _, err := conn.Write([]byte{5, 0, 0, 1, 0, 0, 0, 0, 0, 0}); err != nil {
		return
	}
	_ = conn.SetDeadline(time.Time{})

	done := make(chan struct{}, 2)
	go func() {
		_, _ = io.Copy(upstream, reader)
		done <- struct{}{}
	}()
	go func() {
		_, _ = io.Copy(conn, upstream)
		done <- struct{}{}
	}()
	<-done
}
