// Runs the floorplanner off the page's thread. The anneal is one synchronous WASM call, so it
// can't hear a "stop" message: the page stops it by terminating this worker and keeping the last
// snapshot.
import init, { Board } from '../core/composer_core.js';

const ready = init();

self.onmessage = async (e) => {
  await ready;
  const { problem, poses, seed, moves, warm, every } = e.data;
  const board = new Board(problem);
  board.set_poses(poses);
  const started = performance.now();
  const placement = board.anneal(seed, moves, warm, every, (p, metrics, done, total) => {
    self.postMessage({ type: 'snap', poses: p, metrics, done, total });
    return true;
  });
  self.postMessage({ type: 'done', placement, poses: board.poses(), seconds: (performance.now() - started) / 1000 });
};
