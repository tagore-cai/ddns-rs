/*   Copyright (C) 2022-2026 ddns-rs maintainers */

'use strict';
'require view';
'require rpc';
'require ui';
'require fs';

var callStatus = rpc.declare({
	object: 'luci.ddns-rs',
	method: 'status',
	expect: {}
});

var callInstall = rpc.declare({
	object: 'luci.ddns-rs',
	method: 'binary_install',
	params: [ 'url' ],
	expect: {}
});

var callUploadInstall = rpc.declare({
	object: 'luci.ddns-rs',
	method: 'binary_upload',
	params: [ 'path' ],
	expect: {}
});

var callUpdate = rpc.declare({
	object: 'luci.ddns-rs',
	method: 'binary_update',
	expect: {}
});

var callProgress = rpc.declare({
	object: 'luci.ddns-rs',
	method: 'binary_progress',
	expect: {}
});

function valueOrDash(value) {
	return (value === null || value === undefined || value === '') ? '-' : value;
}

function setText(id, value) {
	var node = document.getElementById(id);
	if (node)
		node.textContent = valueOrDash(value);
}

function replaceContent(node, content) {
	if (!node)
		return;
	while (node.firstChild)
		node.removeChild(node.firstChild);
	if (content === null || content === undefined)
		return;
	if (Array.isArray(content)) {
		for (var i = 0; i < content.length; i++)
			node.appendChild(content[i]);
	} else if (content.nodeType) {
		node.appendChild(content);
	} else {
		node.textContent = String(content);
	}
}

function scrollProgressLog(logNode) {
	if (logNode)
		logNode.scrollTop = logNode.scrollHeight;
}

function renderError(result) {
	var message = (result && (result.message || result.error)) || _('Install failed');
	var children = [ E('p', {}, E('strong', {}, _('Install failed'))) ];
	if (result && result.stage)
		children.push(E('p', {}, [ E('strong', {}, _('Stage: ')), result.stage ]));
	children.push(E('p', {}, [ E('strong', {}, _('Reason: ')), message ]));
	if (result && result.detail)
		children.push(E('p', { 'class': 'small' }, [ E('strong', {}, _('Details: ')), result.detail ]));
	if (result && result.code)
		children.push(E('p', { 'class': 'small' }, [ E('strong', {}, _('Error code: ')), result.code ]));
	return E('div', {}, children);
}

function callWithRpcTimeout(call, seconds) {
	var previousTimeout = L.env.rpctimeout;
	L.env.rpctimeout = seconds;
	return call().then(function(result) {
		if (previousTimeout === undefined)
			delete L.env.rpctimeout;
		else
			L.env.rpctimeout = previousTimeout;
		return result;
	}, function(err) {
		if (previousTimeout === undefined)
			delete L.env.rpctimeout;
		else
			L.env.rpctimeout = previousTimeout;
		throw err;
	});
}

function refreshStatus() {
	return L.resolveDefault(callStatus(), {}).then(function(status) {
		status = status || {};
		var binary = status.binary || {};
		var service = status.service || {};
		setText('ddns-rs-status-installed', binary.installed ? _('Installed') : _('Not installed'));
		setText('ddns-rs-status-version', binary.version);
		setText('ddns-rs-status-path', binary.path);
		setText('ddns-rs-status-service', service.running ? _('Running') : _('Stopped'));
	}).catch(function() {
		setText('ddns-rs-status-installed', _('Status check error'));
	});
}

function showProgressModal(label) {
	var statusNode = E('p', { 'id': 'ddns-rs-progress-status' }, label);
	var logNode = E('pre', {
		'id': 'ddns-rs-progress-log',
		'style': [
			'box-sizing: border-box',
			'width: 100%',
			'min-height: 12em',
			'max-height: 40vh',
			'overflow: auto',
			'padding: 1em',
			'border: 1px solid #ccc',
			'background: #111',
			'color: #eee',
			'white-space: pre-wrap',
			'font-family: monospace',
			'font-size: 12px',
			'line-height: 1.45'
		].join(';')
	}, _('Waiting for command output...'));
	var resultNode = E('div', { 'id': 'ddns-rs-progress-result' });
	var closeButton = E('button', {
		'class': 'btn cbi-button cbi-button-neutral',
		'disabled': 'disabled',
		'click': function(ev) {
			ev.preventDefault();
			ui.hideModal();
		}
	}, _('Close'));

	ui.showModal(_('DDNS-RS Binary'), [
		statusNode,
		E('div', { 'class': 'cbi-value-title', 'style': 'margin-bottom: .35em;' }, _('Command output')),
		logNode,
		resultNode,
		E('div', { 'class': 'right', 'style': 'margin-top: 1em;' }, closeButton)
	]);

	return {
		status: statusNode,
		log: logNode,
		result: resultNode,
		closeButton: closeButton
	};
}

function setProgressFinished(nodes, statusText, resultContent) {
	if (nodes && nodes.status)
		nodes.status.textContent = statusText;
	if (nodes && nodes.result)
		replaceContent(nodes.result, resultContent);
	if (nodes && nodes.closeButton) {
		nodes.closeButton.disabled = false;
		nodes.closeButton.removeAttribute('disabled');
	}
}

function refreshProgressLog(logNode) {
	return L.resolveDefault(callProgress(), null).then(function(result) {
		if (!result || result.ok === false)
			return;
		logNode.textContent = result.text || _('Waiting for command output...');
		scrollProgressLog(logNode);
	});
}

function startProgressPolling(logNode) {
	var timer = null;
	var stopped = false;
	var tick = function() {
		if (stopped)
			return Promise.resolve();
		return refreshProgressLog(logNode);
	};
	timer = window.setInterval(tick, 1000);
	window.setTimeout(tick, 300);
	return {
		stop: function() {
			stopped = true;
			if (timer !== null)
				window.clearInterval(timer);
		},
		refresh: function() {
			return refreshProgressLog(logNode);
		}
	};
}

function runBinaryAction(label, call, timeout) {
	var nodes = showProgressModal(label);
	var progress = startProgressPolling(nodes.log);

	return L.resolveDefault(callWithRpcTimeout(call, timeout), null).then(function(result) {
		progress.stop();
		if (!result || result.ok === false) {
			return progress.refresh().then(function() {
				setProgressFinished(nodes, _('Operation failed'), renderError(result || {}));
			});
		}
		return progress.refresh().then(function() {
			setProgressFinished(nodes, _('Operation completed.'), E('p', {}, _('Operation completed.')));
			ui.addNotification(null, E('p', {}, _('Operation completed.')), 'info');
		});
	}).catch(function(err) {
		progress.stop();
		return progress.refresh().then(function() {
			var errorResult = {
				message: (err && err.message) || String(err)
			};
			setProgressFinished(nodes, _('Operation failed'), renderError(errorResult));
		});
	}).finally(function() {
		refreshStatus();
	});
}

// Upload a local file to /tmp via LuCI's cgi-upload endpoint.
function uploadToRouter(dest) {
	return new Promise(function(resolve, reject) {
		var fileInput = E('input', { 'type': 'file' });

		fileInput.addEventListener('change', function() {
			var file = fileInput.files && fileInput.files[0];
			if (!file) {
				reject(new Error(_('Please select a file to upload')));
				return;
			}

			var data = new FormData();
			data.append('sessionid', rpc.getSessionID());
			data.append('filename', dest);
			data.append('filedata', file);

			var xhr = new XMLHttpRequest();
			xhr.open('POST', L.env.cgi_base + '/cgi-upload', true);
			xhr.onload = function() {
				if (xhr.status == 200)
					resolve(xhr.responseText);
				else
					reject(new Error('%s (%d)'.format(xhr.statusText || _('HTTP error'), xhr.status)));
			};
			xhr.onerror = function() {
				reject(new Error(_('Network error')));
			};
			xhr.send(data);
		});

		fileInput.style.display = 'none';
		document.body.appendChild(fileInput);
		fileInput.click();
		document.body.removeChild(fileInput);
	});
}

return view.extend({
	render: function() {
		var self = this;

		var container = E('div', { 'class': 'cbi-map' });

		// Status card
		var statusCard = E('div', { 'class': 'cbi-section' }, [
			E('h3', {}, _('DDNS-RS Binary Status')),
			E('div', { 'class': 'tr' }, [
				E('div', { 'class': 'td left', 'style': 'width: 240px' }, _('Installed')),
				E('div', { 'class': 'td left', 'id': 'ddns-rs-status-installed' }, _('Loading...'))
			]),
			E('div', { 'class': 'tr' }, [
				E('div', { 'class': 'td left', 'style': 'width: 240px' }, _('Version')),
				E('div', { 'class': 'td left', 'id': 'ddns-rs-status-version' }, '-')
			]),
			E('div', { 'class': 'tr' }, [
				E('div', { 'class': 'td left', 'style': 'width: 240px' }, _('Binary path')),
				E('div', { 'class': 'td left', 'id': 'ddns-rs-status-path' }, '-')
			]),
			E('div', { 'class': 'tr' }, [
				E('div', { 'class': 'td left', 'style': 'width: 240px' }, _('Service')),
				E('div', { 'class': 'td left', 'id': 'ddns-rs-status-service' }, '-')
			])
		]);

		// Install from URL card
		var urlInput = E('input', {
			'type': 'text',
			'id': 'ddns-rs-url',
			'class': 'cbi-input-text',
			'style': 'width: 100%; margin-bottom: 5px',
			'placeholder': 'https://github.com/jeessy2/ddns-rs/releases/...'
		});
		var installBtn = E('button', {
			'class': 'btn cbi-button-action',
			'click': function() {
				var url = urlInput.value.trim();
				if (!url) {
					ui.addNotification(null, E('p', {}, _('Please enter a download URL')), 'danger');
					return;
				}
				runBinaryAction(_('Installing ddns-rs from URL...'), function() {
					return callInstall(url);
				}, 300);
			}
		}, _('Install from URL'));

		var installCard = E('div', { 'class': 'cbi-section' }, [
			E('h3', {}, _('Install from URL')),
			E('p', {}, _('Enter a direct download URL of the ddns-rs binary or .tar.gz archive.')),
			E('div', { 'class': 'cbi-value' }, [ urlInput, installBtn ])
		]);

		// Upload card
		var uploadBtn = E('button', {
			'class': 'btn cbi-button-action',
			'click': function() {
				var uploadPath = '/tmp/ddns-rs-upload-%d'.format(Date.now());
				ui.uploadFile(uploadPath).then(function() {
					return runBinaryAction(_('Installing uploaded ddns-rs binary...'), function() {
						return callUploadInstall(uploadPath);
					}, 300);
				}).catch(function(err) {
					if (err && err.message && err.message.indexOf(_('Upload has been cancelled')) >= 0)
						return;
					ui.addNotification(null, E('p', {}, (err && err.message) || String(err)), 'danger');
				});
			}
		}, _('Upload & Install'));

		var uploadCard = E('div', { 'class': 'cbi-section' }, [
			E('h3', {}, _('Install Binary')),
			E('p', {}, _('Upload a ddns-rs binary or .tar.gz archive.')),
			E('div', { 'class': 'cbi-value' }, [ uploadBtn ])
		]);

		// Auto update card
		var updateBtn = E('button', {
			'class': 'btn cbi-button-action',
			'click': function() {
				runBinaryAction(_('Updating ddns-rs...'), function() {
					return callUpdate();
				}, 300);
			}
		}, _('Auto Install/Update'));

		var updateCard = E('div', { 'class': 'cbi-section' }, [
			E('h3', {}, _('Auto Install / Update')),
			E('p', {}, _('If the binary is missing, it will be downloaded from the default release. '
			             + 'If installed, it will check for the latest version and update automatically.')),
			E('div', { 'class': 'cbi-value' }, [ updateBtn ])
		]);

		container.appendChild(statusCard);
		container.appendChild(uploadCard);
		container.appendChild(installCard);
		container.appendChild(updateCard);

		self.refreshStatus = refreshStatus;
		self.refreshStatus();
		return container;
	}
});
