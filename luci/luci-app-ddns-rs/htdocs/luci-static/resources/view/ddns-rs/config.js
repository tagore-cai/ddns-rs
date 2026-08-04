/*   Copyright (C) 2022-2026 sirpdboy herboy2008@gmail.com*/
'use strict';
'require view';
'require fs';
'require ui';
'require uci';
'require form';
'require poll';
'require rpc';

const getDDNSGoInfo = rpc.declare({
    object: 'luci.ddns-rs',
    method: 'get_ver',
    expect: { 'ver': {} }
});

const getStatusInfo = rpc.declare({
    object: 'luci.ddns-rs',
    method: 'status',
    expect: {}
});

function checkProcess() {
    return L.resolveDefault(getStatusInfo(), {}).then(function(status) {
        status = status || {};
        var binary = status.binary || {};
        return {
            running: !!(status.service && status.service.running),
            version: binary.version
        };
    }).catch(function() {
        return { running: false, version: '' };
    });
}

function getVersionInfo() {
    return L.resolveDefault(getDDNSGoInfo(), {}).then(function(result) {
        var ver = (result && result.ver) || {};
        return { version: ver.version || '' };
    }).catch(function(error) {
        console.error('Failed to get version:', error);
        return {};
    });
}

function extractPortNumber(portValue) {
    if (!portValue) return '9876';
    if (portValue.includes(':')) {
        var parts = portValue.split(':');
        return parts[parts.length - 1];
    }
    return portValue;
}

function renderStatus(isRunning, listen_port, noweb, version) {
    var statusText = isRunning ? _('RUNNING') : _('NOT RUNNING');
    var color = isRunning ? 'green' : 'red';
    var icon = isRunning ? '✓' : '✗';
    var versionText = version ? `v${version}` : '';
    
    var html = String.format(
        '<em><span style="color:%s">%s <strong>%s %s - %s</strong></span></em>',
        color, icon, _('DDNS-RS'), versionText, statusText
    );
    
    if (isRunning) {
        html += String.format('&#160;<a class="btn cbi-button" href="http://%s:%s" target="_blank">%s</a>', 
             window.location.hostname, listen_port, _('Open Web Interface'));
    }
    
    return html;
}

return view.extend({
    load: function() {
        return Promise.all([
            uci.load('ddns-rs')
        ]);
    },
    
    handleResetPassword: async function () {
        try {
            ui.showModal(_('Resetting Password'), [
                E('p', { 'class': 'spinning' }, _('Resetting admin username and password, please wait...'))
            ]);
            const result = await fs.exec('/usr/bin/ddns-rs', ['-resetPassword', 'admin12345', '-c', '/etc/ddns-rs/ddns-rs-config.yaml']);
            const configFile = '/etc/ddns-rs/ddns-rs-config.yaml';
            const readResult = await fs.read(configFile);
            if (readResult && readResult.trim() !== '') {
                let configContent = readResult;
                configContent = configContent.replace(/(username:\s*).*/g, '$1admin');
                
                if (!configContent.includes('user:')) {
                    configContent += '\nuser:\n    username: admin\n    password: $2a$10$G1xO1cVUYtSpPYwV/Jk3l.u7PxLUxo03wntWG6VA9BxAftNWfZEhK';
                }
                
                await fs.write(configFile, configContent);
            }

            ui.hideModal();

            if (result.code === 0) {
                ui.showModal(_('Username and Password Reset Successful'), [
                    E('p', _('Username: admin, Password: admin12345')),
                    E('p', _('You need to restart DDNS-RS service for the changes to take effect.')),
                    E('div', { 'class': 'right' }, [
                        E('button', {
                            'class': 'btn cbi-button cbi-button-positive',
                            'click': ui.createHandlerFn(this, function() {
                                ui.hideModal();
                                this.handleRestartService();
                            })
                        }, _('Restart Service Now')),
                        ' ',
                        E('button', {
                            'class': 'btn cbi-button cbi-button-neutral',
                            'click': ui.hideModal
                        }, _('Restart Later'))
                    ])
                ]);
            } else {
                ui.showModal(_('Partial Reset'), [
                    E('p', _('DDNS-RS command reset may have failed, but configuration file has been updated.')),
                    E('p', _('Username: admin, Password: admin12345')),
                    E('p', _('You may need to restart DDNS-RS service manually.')),
                    E('div', { 'class': 'right' }, [
                        E('button', {
                            'class': 'btn cbi-button cbi-button-positive',
                            'click': ui.createHandlerFn(this, function() {
                                ui.hideModal();
                                this.handleRestartService();
                            })
                        }, _('Restart Service Now')),
                        ' ',
                        E('button', {
                            'class': 'btn cbi-button cbi-button-neutral',
                            'click': ui.hideModal
                        }, _('Close'))
                    ])
                ]);
            }
            
        } catch (error) {
            ui.hideModal();
            alert(_('ERROR:') + '\n' + _('Reset username/password failed:') + '\n' + error.message);
        }
    },
 
    handleRestartService: async function() {
    try {
        // const enabledValue = document.querySelector('input[name="cfg001c48.enabled"]')?.checked ? '1' : '0';
	const enabledValue = document.querySelectorAll('input[id="widget.cbid.ddns-rs.config.enabled"]')?.checked ? '1' : '0';

        await uci.set('ddns-rs', 'config', 'enabled', enabledValue);
        await uci.save('ddns-rs');
        await uci.apply();
        
        await fs.exec('/etc/init.d/ddns-rs', ['stop']);
        await new Promise(resolve => setTimeout(resolve, 1000));
        
        if (enabledValue === '1') {
            await fs.exec('/etc/init.d/ddns-rs', ['start']);
        }
        
        alert(_('SUCCESS:') + '\n' + _('DDNS-RS service restarted successfully'));
        if (window.statusPoll) {
            window.statusPoll();
        }
    } catch (error) {
        alert(_('ERROR:') + '\n' + _('Failed to restart service:') + '\n' + error.message);
    }
    },

    render: function(data) {
        var m, s, o;
        
        var portValue = uci.get('ddns-rs', 'config', 'port') || '[::]:9876';
        var listen_port = extractPortNumber(portValue);
        var noweb = uci.get('ddns-rs', 'config', 'noweb') || '0';

        m = new form.Map('ddns-rs', _('DDNS-RS'),
            _('DDNS-RS automatically obtains your public IPv4 or IPv6 address and resolves it to the corresponding domain name service.'));

        s = m.section(form.TypedSection);
        s.anonymous = true;
   
        s.render = function() {
            var statusView = E('p', { id: 'control_status' }, 
                '<span class="spinning"></span> ' + _('Checking status...'));
            
            window.statusPoll = function() {
                return Promise.all([
                    checkProcess(),
                    getVersionInfo()
                ]).then(function(results) {
                    var [processInfo, versionInfo] = results;
                    var version = versionInfo.version || '';
                    statusView.innerHTML = renderStatus(processInfo.running, listen_port, noweb, version);
                }).catch(function(err) {
                    console.error('Status check failed:', err);
                    statusView.innerHTML = '<span style="color:orange">⚠ ' + _('Status check error') + '</span>';
                });
            };
            
            poll.add(window.statusPoll, 5);
            
            return E('div', { class: 'cbi-section', id: 'status_bar' }, [
                statusView
            ]);
        };

        s = m.section(form.NamedSection, 'config', 'basic');

        o = s.option(form.Flag, 'enabled', _('Enable'));
        o.default = o.disabled;
        o.rmempty = false;

        o = s.option(form.Value, 'port', _('Listen port'));
        o.default = '9876';
        o.rmempty = false;
        o.datatype = 'string'; 
        o.description = _('Port number (1-65535)');

        o = s.option(form.Value, 'time', _('Update interval (seconds)'));
        o.default = '300';
        o.datatype = 'range(60,86400)'; 
        o.description = _('Update interval in seconds (60-86400)');

        o = s.option(form.Value, 'ctimes', _('Provider comparison interval'));
        o.default = '5';
        o.datatype = 'range(1,60)';
        o.description = _('Number of times to compare with service provider (1-60)');

        o = s.option(form.Value, 'skipverify', _('Skip verifying certificates'));
        o.default = '0';
        o.value('0', _('No'));
        o.value('1', _('Yes'));

        o = s.option(form.Value, 'dns', _('Specify DNS resolution server'));
        o.value('223.5.5.5', _('Ali DNS 223.5.5.5'));
        o.value('223.6.6.6', _('Ali DNS 223.6.6.6'));
        o.value('119.29.29.29', _('Tencent DNS 119.29.29.29'));
        o.value('1.1.1.1', _('CloudFlare DNS 1.1.1.1'));
        o.value('8.8.8.8', _('Google DNS 8.8.8.8'));
        o.value('8.8.4.4', _('Google DNS 8.8.4.4'));
        o.datatype = 'ipaddr'; 

        o = s.option(form.Flag, 'noweb', _('Do not start web services'));
        o.default = '0';
        o.rmempty = false;

        o = s.option(form.Value, 'delay', _('Delayed Start (seconds)'));
        o.default = '60';
    
        o = s.option(form.Button, '_newpassword', _('Reset account password'));
        o.inputtitle = _('Reset');
        o.inputstyle = 'apply';
        o.onclick = L.bind(this.handleResetPassword, this, data);

        o = s.option(form.DummyValue, '_version', _('Current Version'));
        o.rawhtml = true;
        
        var currentVersion = '';
	
        getVersionInfo().then(function(versionInfo) {
            currentVersion = versionInfo.version || '';
            var updateView = document.getElementById('update_status');
            if (updateView) {
                updateView.innerHTML = String.format('<span>v%s</span>', currentVersion);
            }
        });
        
        o.cfgvalue = function() {
            return E('div', { style: 'margin: 5px 0;' }, [
                E('span', { id: 'update_status' }, 
                    currentVersion ? String.format('v%s', currentVersion) : _('Loading...'))
            ]);
        };
        
        return m.render();
    }
});