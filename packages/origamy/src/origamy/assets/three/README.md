# three.js

Vendored from the npm package `three` 0.186.1, unmodified, under the MIT license in `LICENSE`.
The files here are `build/three.module.js` and `build/three.core.js`, plus `examples/jsm/controls/OrbitControls.js` at its package path.
The standalone viewer loads the same version from jsDelivr; the tests serve these files to Chromium instead, so they never touch the network.
The constant `origamy.viewer.THREE_VERSION` names the version in one place.

To update: `npm pack three@<version>`, copy these files and `LICENSE` here, and change `THREE_VERSION`.
