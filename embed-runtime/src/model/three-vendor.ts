// three.js as one classic script for `model@1` forks (global `THREE`): the
// whole core plus the two addons the viewer uses, so a fork can reach for
// any other part of three without a build. Declared `vendored` in the fork.
export * from 'three';
export { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
export { OrbitControls } from 'three/addons/controls/OrbitControls.js';
