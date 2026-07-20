// Bench worker for the packed-ArrayBuffer scenario. Receives an ArrayBuffer
// (either structured-cloned or transferred — indistinguishable on the
// receiving side; transfer semantics only affect the sender) and immediately
// acks back with just its byteLength, so the ack payload is trivially small
// and the round-trip time is dominated by the main->worker send of `payload`,
// approximating a one-way postMessage cost. Mirrors worker.mjs's pattern.
import { parentPort } from 'worker_threads';

parentPort.on('message', (msg) => {
  if (msg && msg.type === 'buffer') {
    parentPort.postMessage({ type: 'ack', byteLength: msg.payload.byteLength });
  } else if (msg && msg.type === 'ping') {
    parentPort.postMessage({ type: 'pong' });
  }
});
