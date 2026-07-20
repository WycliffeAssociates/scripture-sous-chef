// Bench worker: receives a Finding[] array via structured clone, immediately
// acks back with just the length (so the ack payload is trivially small and
// the round-trip time is dominated by the main->worker clone of `findings`,
// approximating a one-way postMessage cost).
import { parentPort } from 'worker_threads';

parentPort.on('message', (msg) => {
  if (msg && msg.type === 'findings') {
    parentPort.postMessage({ type: 'ack', n: msg.payload.length });
  } else if (msg && msg.type === 'ping') {
    parentPort.postMessage({ type: 'pong' });
  }
});
