/*   Copyright (C) 2022-2026 ddns-rs maintainers */

'use strict';
'require view';
'require ui';
'require uci';
'require poll';
'require rpc';

var callStatus = rpc.declare({
    object: 'luci.ddns-rs',
    method: 'status',
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

function renderRow(label, value, id) {
    return E('div', { 'class': 'tr' }, [
        E('div', { 'class': 'td left', 'style': 'width: 240px' }, label),
        E('div', { 'class': 'td left', 'id': id }, valueOrDash(value))
    ]);
}

return view.extend({
    load: function() {
        return Promise.all([
            uci.load('ddns-rs'),
            L.resolveDefault(callStatus(), {})
        ]);
    },

    render: function(data) {
        var uciData = data[0];
        var status = data[1] || {};

        var binary = status.binary || {};
        var service = status.service || {};
        var installed = !!binary.installed;
        var running = !!service.running;

        var port = uci.get('ddns-rs', 'config', 'port') || '[::]:9876';
        port = port.split(':').pop();
        var webUrl = 'http://' + window.location.hostname + ':' + port;

        var container = E('div', { 'class': 'cbi-map' });

        var statusCard = E('div', { 'class': 'cbi-section' }, [
            E('h3', {}, _('DDNS-RS Service')),
            renderRow(_('Installed'), installed ? _('Yes') : _('No'), 'ddns-rs-panel-installed'),
            renderRow(_('Version'), binary.version, 'ddns-rs-panel-version'),
            renderRow(_('Service'), running ? _('Running') : _('Stopped'), 'ddns-rs-panel-service')
        ]);

        var openBtn = E('a', {
            'class': 'btn cbi-button cbi-button-action',
            'href': webUrl,
            'target': '_blank',
            'rel': 'noopener'
        }, _('Open Web Interface'));

        var webCard = E('div', { 'class': 'cbi-section' }, [
            E('h3', {}, _('Web Interface')),
            E('p', {}, _('DDNS-RS provides its own web interface. Open it in a new browser tab to manage domains and providers.')),
            E('div', { 'class': 'cbi-value' }, [ openBtn ])
        ]);

        container.appendChild(statusCard);
        container.appendChild(webCard);

        poll.add(function() {
            return L.resolveDefault(callStatus(), {}).then(function(newStatus) {
                var ns = newStatus || {};
                var newRunning = !!(ns.service && ns.service.running);
                if (newRunning !== running) {
                    var el = document.getElementById('ddns-rs-panel-service');
                    if (el)
                        el.textContent = newRunning ? _('Running') : _('Stopped');
                    running = newRunning;
                }
            });
        }, 5);

        poll.start();

        return container;
    }
});
