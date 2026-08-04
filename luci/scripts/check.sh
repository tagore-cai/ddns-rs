#!/bin/sh

set -eu

# Static checks for the ddns-rs LuCI package:
#  1. frontend JS files must parse (no syntax errors)
#  2. JSON files (menu.d / acl.d) must be valid
#  3. every _("...") string and menu title must exist in the .pot and zh_Hans PO

H="luci/luci-app-ddns-rs/htdocs/luci-static/resources/view/ddns-rs"
R="luci/luci-app-ddns-rs/root"

node -e "const fs=require('fs'); for (const f of fs.readdirSync('$H').filter(x=>x.endsWith('.js'))) new Function(fs.readFileSync('$H/'+f,'utf8'));"
node -e "for (const f of ['$R/usr/share/luci/menu.d/luci-app-ddns-rs.json','$R/usr/share/rpcd/acl.d/luci-app-ddns-rs.json']) JSON.parse(require('fs').readFileSync(f,'utf8'));"

node <<'NODE'
const fs = require('fs');

const viewDir = 'luci/luci-app-ddns-rs/htdocs/luci-static/resources/view/ddns-rs';
const jsFiles = fs.readdirSync(viewDir).filter(f => f.endsWith('.js'));

const required = new Set();
for (const file of jsFiles) {
	const source = fs.readFileSync(viewDir + '/' + file, 'utf8');
	const re = /_\(\s*(['"])((?:\\.|[^\\])*?)\1\s*\)/g;
	let match;
	while ((match = re.exec(source)))
		required.add(Function(`return ${match[1]}${match[2]}${match[1]}`)());
}

const menu = JSON.parse(fs.readFileSync('luci/luci-app-ddns-rs/root/usr/share/luci/menu.d/luci-app-ddns-rs.json', 'utf8'));
for (const entry of Object.values(menu)) {
	if (entry.title)
		required.add(entry.title);
}

function poIds(file) {
	const ids = new Set();
	const content = fs.readFileSync(file, 'utf8');
	const re = /^msgid "((?:\\.|[^"\\])*)"$/mg;
	let match;
	while ((match = re.exec(content))) {
		const id = JSON.parse(`"${match[1]}"`);
		if (id)
			ids.add(id);
	}
	return ids;
}

const pot = poIds('luci/luci-app-ddns-rs/po/templates/ddns-rs.pot');
const zh = poIds('luci/luci-app-ddns-rs/po/zh_Hans/ddns-rs.po');
const failures = [];
for (const [label, ids] of [['POT', pot], ['zh_Hans PO', zh]]) {
	for (const id of [...required].sort()) {
		if (!ids.has(id))
			failures.push(`${label} missing msgid: ${id}`);
	}
}
for (const id of [...pot].sort()) {
	if (!required.has(id))
		failures.push(`POT stale msgid: ${id}`);
}
if (failures.length) {
	console.error(failures.join('\n'));
	process.exit(1);
}
console.log('i18n check passed');
NODE
