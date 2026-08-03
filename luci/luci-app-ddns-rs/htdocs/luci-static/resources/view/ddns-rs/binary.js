/*   Copyright (C) 2022-2026 ddns-rs maintainers */

'use strict';
'require view';
'require fs';
'require ui';
'require rpc';

var callService = rpc.declare({
    object: 'luci.ddns-rs',
    method: 'binary_status',
    expect: {}
});

var callInstall = rpc.declare({
    object: 'luci.ddns-rs',
    method: 'binary_install',
    expect: {},
    params: { url: true }
});

var callUpload = rpc.declare({
    object: 'luci.ddns-rs',
    method: 'binary_upload',
    expect: {},
    params: { path: true }
});

var callUpdate = rpc.declare({
    object: 'luci.ddns-rs',
    method: 'binary_update',
    expect: {}
});

function notifyOk(msg) {
    ui.addNotification(null, E('p', {}, msg));
}

function notifyErr(msg) {
    ui.addNotification(null, E('p', { 'style': 'color: red' }, msg));
}

function runInstall(promise, btn) {
    if (btn)
        btn.disabled = true;
    return promise.then(function(res) {
        var text = (res && res.result) ? res.result : _('Install failed');
        if (text.match(/failed|invalid|error/i))
            notifyErr(text);
        else
            notifyOk(text);
        refreshStatus();
    }).catch(function(e) {
        notifyErr(_('Install failed: %s').replace('%s', e));
    }).finally(function() {
        if (btn)
            btn.disabled = false;
    });
}

// Upload a local file to the router's /tmp via LuCI's cgi-upload endpoint.
// Uses a plain XHR so it works regardless of how cgi-upload replies.
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

function refreshStatus() {
    callService().then(function(res) {
        var el = document.getElementById('ddns-rs-status');
        if (!el)
            return;
        var statusText = (res && res.status === 'installed') ? _('Installed') : _('Not installed');
        var verText = (res && res.version && res.version !== 'not-installed') ? res.version : '';
        el.innerHTML = '<strong>' + statusText + '</strong>' + (verText ? ' — ' + verText : '');
    }).catch(function() {
        var el = document.getElementById('ddns-rs-status');
        if (el)
            el.innerHTML = _('Status check error');
    });
}

return view.extend({
    render: function() {
        var self = this;

        var container = E('div', { 'class': 'cbi-map' });

        // Status card
        var statusCard = E('div', { 'class': 'cbi-section' }, [
            E('h3', {}, _('DDNS-RS Binary Status')),
            E('div', { 'id': 'ddns-rs-status', 'class': 'cbi-value' }, _('Loading...'))
        ]);

        // Upload from local file card
        var uploadBtn = E('button', {
            'class': 'btn cbi-button-action',
            'click': function() {
                var uploadPath = '/tmp/ddns-rs-upload-%d'.format(Date.now());
                uploadBtn.disabled = true;
                uploadToRouter(uploadPath).then(function() {
                    return callUpload({ path: uploadPath });
                }).then(function(res) {
                    var text = (res && res.result) ? res.result : _('Install failed');
                    if (text.match(/failed|invalid|error/i))
                        notifyErr(text);
                    else
                        notifyOk(text);
                    refreshStatus();
                }).catch(function(e) {
                    notifyErr(_('Upload failed: %s').replace('%s', e));
                }).finally(function() {
                    uploadBtn.disabled = false;
                });
            }
        }, _('Upload & Install'));

        var uploadCard = E('div', { 'class': 'cbi-section' }, [
            E('h3', {}, _('Install Binary')),
            E('p', {}, _('Upload a ddns-rs binary or .tar.gz archive, or enter a direct download URL below.')),
            E('div', { 'class': 'cbi-value' }, [ uploadBtn ])
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
                    notifyErr(_('Please enter a download URL'));
                    return;
                }
                runInstall(callInstall({ url: url }), installBtn);
            }
        }, _('Install from URL'));

        var installCard = E('div', { 'class': 'cbi-section' }, [
            E('h3', {}, _('Install from URL')),
            E('p', {}, _('Enter a direct download URL of the ddns-rs binary or .tar.gz archive.')),
            E('div', { 'class': 'cbi-value' }, [ urlInput, installBtn ])
        ]);

        // Update card
        var updateBtn = E('button', {
            'class': 'btn cbi-button-action',
            'click': function() {
                runInstall(callUpdate(), updateBtn);
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

        this.refreshStatus = refreshStatus;
        this.refreshStatus();
        return container;
    }
});
