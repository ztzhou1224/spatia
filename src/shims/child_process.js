// Browser shim for Node.js child_process module.
// @loaders.gl/worker-utils imports `spawn` from child_process inside
// child-process-proxy.js, which is a code path only used in Node/worker
// environments. This stub prevents the "spawn is not exported" Vite warning
// without affecting runtime behaviour in the browser (the code path is never
// actually reached in a browser build).
export const spawn = undefined;
export const exec = undefined;
export const execSync = undefined;
export const spawnSync = undefined;
export default {};
