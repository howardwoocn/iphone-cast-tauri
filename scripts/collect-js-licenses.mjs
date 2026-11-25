import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const lock=JSON.parse(fs.readFileSync(path.join(root,'package-lock.json'),'utf8'));
const summary=[];
for(const [relative,entry] of Object.entries(lock.packages)){
  if(!relative||!relative.startsWith('node_modules/'))continue;
  const source=path.join(root,relative);
  if(!fs.existsSync(source))continue;
  const manifest=JSON.parse(fs.readFileSync(path.join(source,'package.json'),'utf8'));
  const destination=path.join(root,'licenses/javascript',`${manifest.name.replaceAll('/','__')}-${manifest.version}`);
  const files=fs.readdirSync(source).filter(name=>/^(licen[sc]e|copying|notice)(\.|$|-)/i.test(name)&&fs.statSync(path.join(source,name)).isFile());
  fs.mkdirSync(destination,{recursive:true});
  for(const file of files)fs.copyFileSync(path.join(source,file),path.join(destination,file));
  summary.push({name:manifest.name,version:manifest.version,license:manifest.license??entry.license??null,repository:manifest.repository??null,licenseFiles:files});
}
fs.writeFileSync(path.join(root,'dependencies/javascript-licenses.json'),JSON.stringify(summary,null,2)+'\n');
console.log(`Collected licenses for ${summary.length} JavaScript packages.`);
