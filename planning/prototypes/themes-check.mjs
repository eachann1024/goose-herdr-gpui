import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const dir=path.dirname(fileURLToPath(import.meta.url));
const html=fs.readFileSync(path.join(dir,'themes.html'),'utf8');
const elements=new Map();
const element=id=>{if(!elements.has(id))elements.set(id,{innerHTML:'',textContent:'',dataset:{},addEventListener(){}});return elements.get(id)};
const context=vm.createContext({document:{querySelector:element,body:{dataset:{}},addEventListener(){}},URL,URLSearchParams,location:{href:'file:///themes.html?variant=A',search:'?variant=A'},history:{replaceState(){}}});
vm.runInContext(html.match(/<script>([\s\S]*)<\/script>/)[1],context);
for(const variant of ['A','B'])for(const page of ['workspace','files','search','settings','assets']){
 vm.runInContext(`variant='${variant}';page='${page}';draw()`,context);
 assert.ok(element('#main').innerHTML.length>100);
 assert.equal(context.document.body.dataset.variant,variant);
}
vm.runInContext("page='workspace';session=true;draw()",context);
assert.ok(element('#main').innerHTML.includes('脱敏静态示例'));
// Inline original SVG bytes: file:// CSS masks must not request local files.
assert.ok(!html.includes('../../resources/icons/'));
const iconSources=vm.runInContext('iconSources',context);
for(const [name,uri] of Object.entries(iconSources)){
 assert.ok(uri.startsWith('data:image/svg+xml;base64,'));
 assert.deepEqual(Buffer.from(uri.split(',')[1],'base64'),fs.readFileSync(path.resolve(dir,'../../resources/icons',name+'.svg')));
}
for(const match of html.matchAll(/url\('([^']+)'\)/g)) assert.ok(match[1].startsWith('data:image/svg+xml;base64,')||match[1]==='${iconSources[name]}');
assert.ok(html.includes('.switcher{position:fixed;'));
assert.ok(html.includes('padding-bottom:80px'));
assert.ok(html.includes('[hidden]{display:none!important}'));
assert.ok(html.includes('focus-visible'));
console.log('PASS: both variants, five screens, session view, offline SVG bytes, fixed switcher, fallback visibility.');
