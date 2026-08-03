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
    expect: {}
});

var callUpdate = rpc.declare({
    object: 'luci.ddns-rs',
    method: 'binary_update',
    expect: {}
});

return view.extend({
    render: function() {
        var self = this;

        var container = E('div', { 'class': 'cbi-map' });

        // Status card
        var statusCard = E('div', { 'class': 'cbi-section' }, [
            E('h3', {}, _('DDNS-RS Binary Status')),
            E('div', { 'id': 'ddns-rs-status', 'class': 'cbi-value' }, _('Loading...'))
        ]);

        // Install from URL card
        var urlInput = E('input', {
            'type': 'text',
            'class': 'cbi-input-text',
            'placeholder': 'https://github.com/jeessy2/ddns-rs/releases/...'
        });
        var installBtn = E('button', {
            'class': 'btn cbi-button-action',
            'click': function() {
                var url = urlInput.value.trim();
                if (!url) {
                    ui.addNotification(null, E('p', {}, _('Please enter a download URL')));
                    return;
                }
                installBtn.disabled = true;
                callInstall({ url: url }).then(function(res) {
                    ui.addNotification(null, E('p', {}, res.result));
                    installBtn.disabled = false;
                    self.refreshStatus();
                });
            }
        }, _('Install from URL'));

        var installCard = E('div', { 'class': 'cbi-section' }, [
            E('h3', {}, _('Install Binary')),
            E('p', {}, _('Enter a direct download URL of the ddns-rs binary or .tar.gz archive.')),
            E('div', { 'class': 'cbi-value' }, [ urlInput, installBtn ])
        ]);

        // Update card
        var updateBtn = E('button', {
            'class': 'btn cbi-button-action',
            'click': function() {
                updateBtn.disabled = true;
                callUpdate().then(function(res) {
                    ui.addNotification(null, E('p', {}, res.result));
                    updateBtn.disabled = false;
                    self.refreshStatus();
                });
            }
        }, _('Auto Install/Update'));

        var updateCard = E('div', { 'class': 'cbi-section' }, [
            E('h3', {}, _('Auto Install / Update')),
            E('p', {}, _('If the binary is missing, it will be downloaded from the default release. '
                         + 'If installed, it will check for the latest version and update automatically.')),
            E('div', { 'class': 'cbi-value' }, [ updateBtn ])
        ]);

        container.appendChild(statusCard);
        container.appendChild(installCard);
        container.appendChild(updateCard);

        this.refreshStatus = function() {
            callService().then(function(res) {
                var el = document.getElementById('ddns-rs-status');
                if (el) {
                    var statusText = res.status === 'installed' ? _('Installed') : _('Not installed');
                    var verText = res.version && res.version !== 'not-installed' ? res.version : '';
                    el.innerHTML = '<strong>' + statusText + '</strong>' + (verText ? ' — ' + verText : '');
                }
            });
        };

        this.refreshStatus();
        return container;
    }
});
