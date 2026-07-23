// Bench worker: acks every payload with a trivially small message so the
// round-trip is dominated by the main->worker send. Handles both the object
// array (dev, structured clone) and the packed Uint8Array (transfer) cases.
import { parentPort } from 'worker_threads';

parentPort.on('message', (msg) => {
  if (msg && msg.type === 'findings') {
    parentPort.postMessage({ type: 'ack', n: msg.payload.length });
  } else if (msg && msg.type === 'bytes') {
    parentPort.postMessage({ type: 'ack', n: msg.payload.byteLength });
  } else if (msg && msg.type === 'ping') {
    parentPort.postMessage({ type: 'pong' });
  }
});
