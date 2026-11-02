import http from 'http';

const healthServer = http.createServer((req, res) => {
  if (req.url === '/healthz') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      status: 'UP',
      jurisdiction: 'CH_DLT_ART_973D',
      timestamp: new Date().toISOString(),
      service: 'Helvetia-Compliance-Listener'
    }));
  } else {
    res.writeHead(404);
    res.end();
  }
});

healthServer.listen(process.env.PORT || 8080, () => {
  console.log(`[Healthcheck] Liveness probe listening on port ${process.env.PORT || 8080}`);
});