/** Fades an element in the first time it scrolls into view. */
export function reveal(node, delay = 0) {
	node.classList.add('reveal');
	node.style.setProperty('--reveal-delay', `${delay}ms`);
	if (typeof IntersectionObserver === 'undefined') {
		node.classList.add('shown');
		return;
	}
	const io = new IntersectionObserver(
		(entries) => {
			for (const e of entries) {
				if (e.isIntersecting) {
					node.classList.add('shown');
					io.disconnect();
				}
			}
		},
		{ rootMargin: '0px 0px -8% 0px' }
	);
	io.observe(node);
	return { destroy: () => io.disconnect() };
}
