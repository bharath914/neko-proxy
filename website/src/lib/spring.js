/**
 * A spring described the way Apple describes them: a damping ratio (1 = no
 * overshoot, lower = bouncier) and a response in seconds (lower = snappier).
 * It always animates from its current value and velocity, so a new target
 * mid-flight bends the motion instead of restarting it.
 */
export function createSpring(initial, { response = 0.4, damping = 1, epsilon = 0.05, onUpdate }) {
	let x = initial;
	let v = 0;
	let target = initial;
	let k = 0;
	let c = 0;
	let raf = 0;
	let last = 0;

	const tune = (r, d) => {
		k = (2 * Math.PI / r) ** 2;
		c = (4 * Math.PI * d) / r;
	};
	tune(response, damping);

	function frame(now) {
		const dt = Math.min(0.064, (now - last) / 1000);
		last = now;
		// Fixed sub-steps keep stiff springs stable at any frame rate.
		const steps = Math.max(1, Math.ceil(dt / 0.004));
		const h = dt / steps;
		for (let i = 0; i < steps; i++) {
			const a = -k * (x - target) - c * v;
			v += a * h;
			x += v * h;
		}
		if (Math.abs(x - target) < epsilon && Math.abs(v) < epsilon * 10) {
			x = target;
			v = 0;
			raf = 0;
			onUpdate(x);
			return;
		}
		onUpdate(x);
		raf = requestAnimationFrame(frame);
	}

	return {
		get value() { return x; },
		get target() { return target; },
		/** Move toward `to`. `velocity` (units/s) hands off a gesture's speed. */
		to(to, opts = {}) {
			target = to;
			if (opts.velocity !== undefined) v = opts.velocity;
			tune(opts.response ?? response, opts.damping ?? damping);
			if (!raf) {
				last = performance.now();
				raf = requestAnimationFrame(frame);
			}
		},
		/** Jump with no motion, e.g. while a finger is dragging. */
		set(value) {
			cancelAnimationFrame(raf);
			raf = 0;
			x = target = value;
			v = 0;
			onUpdate(x);
		},
		stop() {
			cancelAnimationFrame(raf);
			raf = 0;
		}
	};
}

/** Where a flick would come to rest, using scroll-view deceleration. */
export function project(velocity, rate = 0.998) {
	return ((velocity / 1000) * rate) / (1 - rate);
}

/** Resistance past an edge: the further you pull, the less it follows. */
export function rubberband(overshoot, dimension, constant = 0.55) {
	return (overshoot * dimension * constant) / (dimension + constant * Math.abs(overshoot));
}

export const reducedMotion = () => typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;
