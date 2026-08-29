// Minimal MiMo-compatible mock upstream for e2e.
// Streaming contract per official docs: pcm16 base64 deltas + `data: [DONE]`.
import http from 'node:http';

const PORT = Number(process.env.MOCK_PORT || 30250);
const PCM = Buffer.alloc(24000); // 0.5s @ 24kHz mono pcm16 (silence)
const B64 = PCM.toString('base64');

const server = http.createServer((req, res) => {
  if (req.method === 'GET' && (req.url || '') === '/health') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end('{"ok":true}');
    return;
  }
  if (req.method === 'POST' && (req.url || '').endsWith('/v1/chat/completions')) {
    res.writeHead(200, { 'Content-Type': 'text/event-stream' });
    res.write(`data: {"choices":[{"delta":{"audio":{"data":"${B64}"}}}]}\n\n`);
    res.write('data: [DONE]\n\n');
    res.end();
    return;
  }
  res.writeHead(404, { 'Content-Type': 'application/json' });
  res.end('{"error":"not found"}');
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`mock-mimo up on ${PORT}`);
});
