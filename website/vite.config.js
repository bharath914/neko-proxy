import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
		sveltekit({
			adapter: adapter({ pages: 'build', assets: 'build', strict: true }),
			// GitHub Pages serves the project at /<repo>; the workflow passes it in.
			paths: { base: /** @type {'' | `/${string}`} */ (process.env.BASE_PATH ?? '') }
		})
	]
});
