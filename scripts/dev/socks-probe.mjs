// Minimal SOCKS5 client used to verify the channel's proxy endpoint.
// Usage: node scripts/dev/socks-probe.mjs 127.0.0.1:PORT host port
import net from "node:net";

const [proxyAddr, targetHost, targetPort] = process.argv.slice(2);
if (!proxyAddr || !targetHost || !targetPort) {
  console.error("usage: node socks-probe.mjs <proxy host:port> <target host> <target port>");
  process.exit(2);
}

const [proxyHost, proxyPort] = proxyAddr.split(":");
const host = Buffer.from(targetHost, "utf8");
const port = Number(targetPort);

const socket = net.connect(Number(proxyPort), proxyHost);
const timeout = setTimeout(() => {
  console.log("timeout");
  socket.destroy();
  process.exit(1);
}, 8000);

let stage = "greeting";

socket.on("connect", () => {
  socket.write(Buffer.from([5, 1, 0]));
});

socket.on("data", (buf) => {
  if (stage === "greeting") {
    console.log("greeting reply:", [...buf].map((b) => "0x" + b.toString(16)).join(" "));
    if (buf[0] !== 5 || buf[1] !== 0) {
      console.log("RESULT: proxy refused no-auth method");
      clearTimeout(timeout);
      socket.destroy();
      return;
    }
    stage = "request";
    const req = Buffer.concat([
      Buffer.from([5, 1, 0, 3, host.length]),
      host,
      Buffer.from([port >> 8, port & 0xff]),
    ]);
    socket.write(req);
    return;
  }

  console.log("connect reply:", [...buf.subarray(0, 10)].map((b) => "0x" + b.toString(16)).join(" "));
  const rep = buf[1];
  const meanings = {
    0: "succeeded",
    1: "general failure",
    2: "not allowed by ruleset",
    3: "network unreachable",
    4: "host unreachable",
    5: "connection refused",
    6: "TTL expired",
    7: "command not supported",
    8: "address type not supported",
  };
  console.log(`RESULT: rep=${rep} (${meanings[rep] || "unknown"})`);
  clearTimeout(timeout);
  socket.destroy();
  process.exit(rep === 0 ? 0 : 1);
});

socket.on("error", (err) => {
  console.log("socket error:", err.message);
  clearTimeout(timeout);
  process.exit(1);
});
