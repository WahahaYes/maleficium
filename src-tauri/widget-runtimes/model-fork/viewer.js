(function(three, three_addons_loaders_GLTFLoader_js, three_addons_controls_OrbitControls_js, src_widget_runtimes_bridge) {
	//#region src/widget-runtimes/model/core.ts
	var GLB_MAGIC = 1179937895;
	var JSON_CHUNK = 1313821514;
	var UNSUPPORTED = [
		"KHR_draco_mesh_compression",
		"EXT_meshopt_compression",
		"KHR_meshopt_compression",
		"KHR_texture_basisu"
	];
	/** Why these bytes cannot be shown, or null when they look like a viewable glb. */
	function checkGlb(bytes) {
		if (bytes.byteLength < 20) return "the model file is too small to be a glb";
		const v = new DataView(bytes);
		if (v.getUint32(0, true) !== GLB_MAGIC) return "the model is not a binary glTF (.glb) file";
		if (v.getUint32(4, true) !== 2) return "only glTF 2.0 models are supported";
		if (v.getUint32(8, true) > bytes.byteLength) return "the model file is truncated";
		const jsonLen = v.getUint32(12, true);
		if (v.getUint32(16, true) !== JSON_CHUNK || 20 + jsonLen > bytes.byteLength) return "the model file has no readable glTF header";
		let doc;
		try {
			doc = JSON.parse(new TextDecoder().decode(new Uint8Array(bytes, 20, jsonLen)));
		} catch {
			return "the model file has no readable glTF header";
		}
		const required = doc.extensionsRequired;
		if (Array.isArray(required)) {
			const bad = required.filter((e) => typeof e === "string" && UNSUPPORTED.includes(e));
			if (bad.length) return `the model needs ${bad[0]}, which the offline viewer cannot decode`;
		}
		return null;
	}
	/** Camera distance that fits a bounding sphere of `radius` in a view of `fovDeg`, and clip planes. */
	function frame(radius, fovDeg, aspect) {
		const r = radius > 0 && Number.isFinite(radius) ? radius : 1;
		const half = fovDeg * Math.PI / 360;
		const hfov = Math.atan(Math.tan(half) * Math.max(aspect, .01));
		const distance = r / Math.sin(Math.min(half, hfov)) * 1.1;
		return {
			distance,
			near: distance / 100,
			far: distance * 100
		};
	}
	/**
	* The `camera` option: a camera-to-world matrix, 16 numbers in column-major
	* order (glTF's and three.js's), as the app canonicalizes it (space
	* separated). Null when absent or malformed: the view then frames the model.
	*/
	function parseCamera(v) {
		const m = (Array.isArray(v) ? v : typeof v === "string" ? v.split(/[\s,]+/).filter((t) => t !== "") : []).map((t) => typeof t === "number" ? t : Number(t));
		if (m.length !== 16 || !m.every(Number.isFinite)) return null;
		const len = (i) => Math.hypot(m[i], m[i + 1], m[i + 2]);
		return len(4) > 1e-9 && len(8) > 1e-9 ? m : null;
	}
	/** The `size` option, `WxH` in pixels (16 to 4096 a side); null otherwise. */
	function parseSize(v) {
		const m = typeof v === "string" ? /^(\d{1,4})x(\d{1,4})$/.exec(v.trim()) : null;
		if (!m) return null;
		const [width, height] = [Number(m[1]), Number(m[2])];
		const ok = (n) => n >= 16 && n <= 4096;
		return ok(width) && ok(height) ? {
			width,
			height
		} : null;
	}
	/** The `background` option: `transparent` or `#rrggbb`; null otherwise. */
	function parseBackground(v) {
		if (typeof v !== "string") return null;
		const t = v.trim().toLowerCase();
		return t === "transparent" || /^#[0-9a-f]{6}$/.test(t) ? t : null;
	}
	/** Clip planes for a camera `distance` from the centre of a bounding sphere of `radius`. */
	function clip(distance, radius) {
		const r = radius > 0 && Number.isFinite(radius) ? radius : 1;
		const d = Number.isFinite(distance) ? Math.max(distance, 0) : r * 3;
		return {
			near: Math.max(d - r * 1.5, r / 1e3, d / 1e3),
			far: d + r * 2
		};
	}
	//#endregion
	//#region src/widget-runtimes/model/main.ts
	var FOV = 40;
	var host = document.getElementById("view");
	var note = document.getElementById("msg");
	var renderer = null;
	var controls = null;
	var model = null;
	var view = {
		camera: null,
		size: null,
		background: null
	};
	var scene = new three.Scene();
	var camera = new three.PerspectiveCamera(FOV, 1, .1, 1e3);
	camera.add(new three.DirectionalLight(16777215, 2.2));
	scene.add(camera, new three.HemisphereLight(16777215, 9079445, 1.6));
	function fail(message) {
		note.hidden = false;
		note.textContent = message;
		(0, src_widget_runtimes_bridge.status)("error", message);
	}
	function background() {
		return view.background ?? getComputedStyle(document.body).backgroundColor;
	}
	function render() {
		renderer?.render(scene, camera);
	}
	function resize() {
		if (!renderer) return;
		const w = Math.max(host.clientWidth, 1);
		const h = Math.max(host.clientHeight, 1);
		renderer.setSize(w, h, false);
		camera.aspect = w / h;
		camera.updateProjectionMatrix();
		render();
	}
	function ensureRenderer() {
		if (renderer) return renderer;
		const r = new three.WebGLRenderer({
			antialias: true,
			alpha: true
		});
		r.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
		r.setClearColor(0, 0);
		r.domElement.tabIndex = 0;
		host.prepend(r.domElement);
		const c = new three_addons_controls_OrbitControls_js.OrbitControls(camera, r.domElement);
		c.enableDamping = false;
		c.listenToKeyEvents(r.domElement);
		c.addEventListener("change", render);
		renderer = r;
		controls = c;
		return r;
	}
	function dispose(o) {
		o.traverse((n) => {
			const m = n;
			m.geometry?.dispose();
			for (const mat of [].concat(m.material ?? [])) mat.dispose();
		});
	}
	/** Places the camera where the `camera` option says, orbiting the model's nearest point ahead. */
	function place(m, sphere) {
		new three.Matrix4().fromArray(m).decompose(camera.position, camera.quaternion, new three.Vector3());
		const forward = new three.Vector3(0, 0, -1).applyQuaternion(camera.quaternion);
		const toCenter = sphere.center.clone().sub(camera.position);
		const c = clip(toCenter.length(), sphere.radius);
		camera.near = c.near;
		camera.far = c.far;
		camera.updateProjectionMatrix();
		const ahead = toCenter.dot(forward);
		controls.target.copy(camera.position).addScaledVector(forward, ahead > 1e-6 ? ahead : Math.max(toCenter.length(), 1));
		controls.update();
	}
	function fit(obj) {
		const sphere = new three.Box3().setFromObject(obj).getBoundingSphere(new three.Sphere());
		if (view.camera) return place(view.camera, sphere);
		const center = sphere.center;
		const f = frame(sphere.radius, FOV, camera.aspect);
		camera.near = f.near;
		camera.far = f.far;
		camera.position.copy(center).add(new three.Vector3(.6, .45, 1).normalize().multiplyScalar(f.distance));
		camera.updateProjectionMatrix();
		controls.target.copy(center);
		controls.update();
	}
	function parse(bytes) {
		return new Promise((resolve, reject) => {
			const g = globalThis;
			const saved = g.createImageBitmap;
			g.createImageBitmap = void 0;
			try {
				new three_addons_loaders_GLTFLoader_js.GLTFLoader().parse(bytes, "", (gltf) => resolve(gltf.scene), (e) => reject(e instanceof Error ? e : /* @__PURE__ */ new Error("the model could not be parsed")));
			} catch (e) {
				reject(e instanceof Error ? e : /* @__PURE__ */ new Error("the model could not be parsed"));
			} finally {
				g.createImageBitmap = saved;
			}
		});
	}
	async function init(msg) {
		view = {
			camera: parseCamera(msg.options.camera),
			size: parseSize(msg.options.size),
			background: parseBackground(msg.options.background)
		};
		if (view.camera) camera.up.fromArray(view.camera, 4).normalize();
		if (view.background) document.body.style.background = view.background;
		if (view.size) host.style.aspectRatio = `${view.size.width} / ${view.size.height}`;
		(0, src_widget_runtimes_bridge.applyTheme)(msg.theme);
		host.setAttribute("role", "img");
		host.setAttribute("aria-label", msg.alt);
		(0, src_widget_runtimes_bridge.status)("loading");
		try {
			const src = msg.sources.model;
			if (!src) return fail("the model widget has no \"model\" source");
			const bad = checkGlb(src.bytes);
			if (bad) return fail(bad);
			const loaded = await parse(src.bytes);
			const r = ensureRenderer();
			if (model) {
				scene.remove(model);
				dispose(model);
			}
			model = loaded;
			scene.add(model);
			r.domElement.setAttribute("aria-label", msg.alt);
			resize();
			fit(model);
			render();
			note.hidden = true;
			(0, src_widget_runtimes_bridge.status)("loaded");
		} catch (err) {
			fail(err instanceof Error ? err.message : "the model failed to load");
		}
	}
	/** The current view over the background, as a PNG data URL, `size` pixels when set. */
	function snapshot() {
		if (!renderer || !model) return null;
		const r = renderer;
		const ratio = r.getPixelRatio();
		if (view.size) {
			r.setPixelRatio(1);
			r.setSize(view.size.width, view.size.height, false);
			camera.aspect = view.size.width / view.size.height;
			camera.updateProjectionMatrix();
		}
		render();
		const src = r.domElement;
		const c = document.createElement("canvas");
		c.width = src.width;
		c.height = src.height;
		const ctx = c.getContext("2d");
		if (ctx) {
			const bg = background();
			if (bg !== "transparent") {
				ctx.fillStyle = new three.Color(bg).getStyle();
				ctx.fillRect(0, 0, c.width, c.height);
			}
			ctx.drawImage(src, 0, 0);
		}
		if (view.size) {
			r.setPixelRatio(ratio);
			resize();
		}
		return ctx ? c.toDataURL("image/png") : null;
	}
	(0, src_widget_runtimes_bridge.startBridge)({
		onInit: (m) => void init(m),
		onTheme: (t) => {
			(0, src_widget_runtimes_bridge.applyTheme)(t);
		},
		onSnapshot: snapshot
	});
	window.addEventListener("resize", resize);
	new ResizeObserver(resize).observe(host);
	//#endregion
})(THREE, THREE, THREE, mfwBridge);
