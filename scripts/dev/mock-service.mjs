// Dev-only mock service: serves a fake management WebUI on a local port so the
// desktop shell can be exercised end-to-end (load, cookie, WebSocket) without
// touching a real server. Also exposes a tiny WebSocket echo endpoint.
//
//   node scripts/dev/mock-service.mjs 6185
import { createServer } from "node:http";
import { readFileSync } from "node:fs";

const loginHtml = readFileSync(new URL("./mock-login.html", import.meta.url), "utf8");
import { createHash } from "node:crypto";

const port = Number(process.argv[2] || 6185);

const page = `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<title>AstrBot 模拟控制台</title>
<style>
  :root { color-scheme: dark; }
  body { margin:0; font-family: "Segoe UI", "Microsoft YaHei UI", sans-serif;
         background:#0b1220; color:#e6edf7; display:flex; min-height:100vh;
         align-items:center; justify-content:center; }
  .box { width:min(760px,86%); }
  h1 { font-size:26px; margin:0 0 6px; }
  .sub { color:#8fa3c0; font-size:13px; margin-bottom:22px; }
  .row { display:flex; gap:10px; flex-wrap:wrap; }
  .card { flex:1 1 200px; background:#121b2c; border:1px solid #22304a;
          border-radius:12px; padding:14px 16px; }
  .k { font-size:11px; color:#7f93b0; letter-spacing:.06em; text-transform:uppercase; }
  .v { font-size:15px; margin-top:6px; font-weight:600; }
  .ok { color:#4dd4ac; } .bad { color:#ff8080; }
  .mock { display:inline-block; margin-top:18px; font-size:11px; color:#5b8cff;
          border:1px dashed #2c4a86; border-radius:8px; padding:6px 10px; }
</style>
</head>
<body>
  <div class="box">
    <h1>模拟 WebUI · 端口 ${port}</h1>
    <div class="sub">这是用于验证桌面壳内嵌加载的假服务页面。真实场景下这里是 AstrBot / NapCat / 1Panel / 哪吒 自己的界面。</div>
    <div class="row">
      <div class="card"><div class="k">WebView2 渲染</div><div class="v ok" id="render">…</div></div>
      <div class="card"><div class="k">WebSocket</div><div class="v" id="ws">连接中…</div></div>
      <div class="card"><div class="k">Cookie</div><div class="v" id="cookie">…</div></div>
      <div class="card"><div class="k">文件上传</div><div class="v" id="upload">待测试</div></div>
      <div class="card"><div class="k">Tauri IPC（应为拒绝）</div><div class="v" id="ipc">探测中…</div></div>
      <div class="card"><div class="k">跨域子资源（是否走通道）</div><div class="v" id="subres">探测中…</div></div>
    </div>
    <div class="mock">MOCK SERVICE · ${port}</div>
  </div>
<script>
  document.getElementById('render').textContent = '正常';

  document.cookie = 'mock_session=webview-ok; max-age=86400; path=/';
  var m = document.cookie.match(/mock_session=([^;]+)/);
  document.getElementById('cookie').textContent = m ? m[1] : '未设置';

  try {
    var ws = new WebSocket('ws://' + location.host + '/ws');
    ws.onopen = function () {
      ws.send('ping');
    };
    ws.onmessage = function (e) {
      var el = document.getElementById('ws');
      el.textContent = e.data === 'pong' ? '正常' : ('回显: ' + e.data);
      el.className = 'v ok';
    };
    ws.onerror = function () {
      var el = document.getElementById('ws');
      el.textContent = '失败';
      el.className = 'v bad';
    };
  } catch (e) {
    document.getElementById('ws').textContent = '不支持';
  }

  var input = document.createElement('input');
  input.type = 'file';
  input.style.display = 'none';
  input.addEventListener('change', function (e) {
    var f = e.target.files && e.target.files[0];
    document.getElementById('upload').textContent = f ? f.name : '未选择';
  });
  document.body.appendChild(input);
  document.getElementById('upload').textContent = '组件就绪';

  // A cross-origin subresource. The hostname only resolves inside the channel,
  // so a successful load proves subresource traffic also goes through it.
  (function () {
    var el = document.getElementById('subres');
    var img = new Image();
    img.onload = function () {
      el.textContent = '经通道可达';
      el.className = 'v bad';
    };
    img.onerror = function () {
      el.textContent = '不可达（未走通道或解析失败）';
      el.className = 'v ok';
    };
    img.src = 'http://probe.tailnet.test:6185/pixel.png';
  })();

  // Probe whether a remote page can reach the Tauri backend. Expected: rejected.
  (async function () {
    var el = document.getElementById('ipc');
    var internals = window.__TAURI_INTERNALS__;
    if (!internals || typeof internals.invoke !== 'function') {
      el.textContent = '不可用（无桥）';
      el.className = 'v ok';
      return;
    }
    try {
      var cfg = await internals.invoke('get_config');
      el.textContent = '可调用！' + JSON.stringify(cfg).slice(0, 40);
      el.className = 'v bad';
    } catch (e) {
      el.textContent = '已拒绝：' + String(e).slice(0, 46);
      el.className = 'v ok';
    }
  })();
</script>
</body>
</html>`;

const PIXEL = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=",
  "base64",
);

const server = createServer((req, res) => {
  if (req.url === "/ws") {
    res.writeHead(400);
    res.end("websocket only");
    return;
  }
  if (req.url === "/login") {
    res.writeHead(200, {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "no-store",
    });
    res.end(loginHtml);
    return;
  }
  if (req.url === "/pixel.png") {
    res.writeHead(200, { "content-type": "image/png", "cache-control": "no-store" });
    res.end(PIXEL);
    return;
  }
  res.writeHead(200, {
    "content-type": "text/html; charset=utf-8",
    "cache-control": "no-store",
  });
  res.end(page);
});

// Minimal RFC6455 echo (text frames only), enough to prove WS works in WebView2.
server.on("upgrade", (req, socket) => {
  const key = req.headers["sec-websocket-key"];
  if (!key) {
    socket.destroy();
    return;
  }
  const accept = createHash("sha1")
    .update(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11")
    .digest("base64");
  socket.write(
    "HTTP/1.1 101 Switching Protocols\r\n" +
      "Upgrade: websocket\r\n" +
      "Connection: Upgrade\r\n" +
      `Sec-WebSocket-Accept: ${accept}\r\n\r\n`,
  );

  socket.on("data", (buf) => {
    if (buf.length < 2) return;
    const opcode = buf[0] & 0x0f;
    if (opcode === 0x8) {
      socket.end();
      return;
    }
    const masked = (buf[1] & 0x80) !== 0;
    let len = buf[1] & 0x7f;
    let offset = 2;
    if (len === 126) {
      len = buf.readUInt16BE(2);
      offset = 4;
    } else if (len === 127) {
      len = Number(buf.readBigUInt64BE(2));
      offset = 10;
    }
    let mask;
    if (masked) {
      mask = buf.subarray(offset, offset + 4);
      offset += 4;
    }
    const payload = Buffer.alloc(len);
    for (let i = 0; i < len; i++) {
      payload[i] = masked
        ? buf[offset + i] ^ mask[i % 4]
        : buf[offset + i];
    }
    const text = payload.toString("utf8");
    const reply = Buffer.from(text === "ping" ? "pong" : text, "utf8");
    const header =
      reply.length < 126
        ? Buffer.from([0x81, reply.length])
        : Buffer.from([0x81, 126, reply.length >> 8, reply.length & 0xff]);
    socket.write(Buffer.concat([header, reply]));
  });
});

server.listen(port, "127.0.0.1", () => {
  console.log(`mock service on http://127.0.0.1:${port}`);
});
