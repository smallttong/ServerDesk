package main

import (
	"io"
	"net"
	"testing"
)

func TestTargetKeyAndAllowlist(t *testing.T) {
	list, err := parseAllowedTargets([]string{"NAS.TAILNET.TEST.:6185", "[fd7a:115c:a1e0::1]:443"})
	if err != nil {
		t.Fatal(err)
	}
	for _, target := range []string{"nas.tailnet.test:6185", "[fd7a:115c:a1e0::1]:443"} {
		key, err := targetKey(target)
		if err != nil {
			t.Fatal(err)
		}
		if _, ok := list[key]; !ok {
			t.Fatalf("missing allowed target %s", key)
		}
	}
	if _, ok := list["nas.tailnet.test:22"]; ok {
		t.Fatal("unexpected port allowed")
	}
	for _, target := range []string{"nas.tailnet.test:0", "nas.tailnet.test:65536", "bad-target"} {
		if _, err := targetKey(target); err == nil {
			t.Fatalf("accepted invalid target %s", target)
		}
	}
}

func TestSOCKSDeniesUnlistedTargetBeforeDial(t *testing.T) {
	allowedTargets.Store(map[string]struct{}{})
	client, server := net.Pipe()
	defer client.Close()
	go handleSOCKS(nil, nil, server)
	if _, err := client.Write([]byte{5, 1, 0}); err != nil {
		t.Fatal(err)
	}
	greeting := make([]byte, 2)
	if _, err := io.ReadFull(client, greeting); err != nil {
		t.Fatal(err)
	}
	if greeting[1] != 0 {
		t.Fatalf("SOCKS greeting failed: %v", greeting)
	}
	host := []byte("nas.tailnet.test")
	request := append([]byte{5, 1, 0, 3, byte(len(host))}, host...)
	request = append(request, 0x18, 0x21) // port 6177, not allowed
	if _, err := client.Write(request); err != nil {
		t.Fatal(err)
	}
	reply := make([]byte, 10)
	if _, err := io.ReadFull(client, reply); err != nil {
		t.Fatal(err)
	}
	if reply[1] != 2 {
		t.Fatalf("expected SOCKS rule rejection, got %v", reply)
	}
}

func TestAllowlistUpdateReplacesRatherThanExtends(t *testing.T) {
	allowedTargets.Store(map[string]struct{}{"old.tailnet.test:80": {}})
	if !applyAllowlistUpdate([]byte(`{"allowedTargets":["new.tailnet.test:443"]}`)) {
		t.Fatal("valid update rejected")
	}
	list := allowedTargets.Load().(map[string]struct{})
	if len(list) != 1 {
		t.Fatalf("old target retained: %v", list)
	}
	if _, ok := list["new.tailnet.test:443"]; !ok {
		t.Fatal("new target missing")
	}
	if applyAllowlistUpdate([]byte(`{"allowedTargets":["bad-target"]}`)) {
		t.Fatal("invalid update accepted")
	}
	if len(allowedTargets.Load().(map[string]struct{})) != 1 {
		t.Fatal("invalid update changed policy")
	}
}
