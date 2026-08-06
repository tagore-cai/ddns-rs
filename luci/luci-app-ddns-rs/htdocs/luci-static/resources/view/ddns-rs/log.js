/*   Copyright (C) 2026 ddns-rs maintainers
 *
 * Log page - mounts the Vite-built Vue app (page: log).
 */

'use strict';
'require view';

function loadScript(src) {
	return new Promise(function(resolve, reject) {
		var s = document.createElement('script');
		s.src = src;
		s.onload = resolve;
		s.onerror = function() { reject(new Error('failed to load ' + src)); };
		document.head.appendChild(s);
	});
}

function loadCss(href) {
	if (document.querySelector('link[href="' + href + '"]'))
		return;
	var l = document.createElement('link');
	l.rel = 'stylesheet';
	l.href = href;
	document.head.appendChild(l);
}

return view.extend({
	render: function() {
		var container = E('div', { 'id': 'ddns-rs-app' });

		loadCss(L.url('resources/ddns-rs-app/ddns-rs-app.css'));
		loadScript(L.url('resources/ddns-rs-app/ddns-rs-app.js')).then(function() {
			if (window.__DDNS_RS_APP__ && typeof window.__DDNS_RS_APP__.mount === 'function')
				window.__DDNS_RS_APP__.mount(container, 'log');
		}).catch(function(err) {
			var el = E('p', { 'class': 'alert-message danger' }, String(err));
			container.appendChild(el);
		});

		return container;
	}
});
