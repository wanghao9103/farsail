// Publish only a signed, validated Android release. Desktop feed is never touched.
import fs from 'node:fs';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
const repo='wanghao9103/farsail';
const gh=(...args)=>execFileSync('gh',args,{encoding:'utf8',stdio:['pipe','pipe','pipe']});
const api=(route,...args)=>JSON.parse(gh('api',`repos/${repo}/${route}`,...args));
const version=JSON.parse(fs.readFileSync('apps/android/version.json'));
if(!/^\d+\.\d+\.\d+$/.test(version.version)||!Number.isSafeInteger(version.versionCode)||version.versionCode<1)throw Error('Invalid Android release version');
const source=process.env.GITHUB_SHA||gh('api',`repos/${repo}/git/ref/heads/codex/android-app-updates`,'--jq','.object.sha').trim();
if(!/^[a-f0-9]{40}$/.test(source))throw Error('Exact source revision required');
const tag=`android-v${version.version}`;
const expected=process.env.RELEASE_TAG||tag;if(expected!==tag)throw Error('Android tag/version mismatch');
const name=`FarSail_Android_${version.version}_universal.apk`;
const input=process.argv[2]||'apps/android/app/build/outputs/apk/release/app-release.apk';
const bytes=fs.readFileSync(input);
const signer=execFileSync(`${process.env.ANDROID_HOME}/build-tools/35.0.0/apksigner`,['verify','--print-certs',input],{encoding:'utf8'});
if(!signer.includes(`Signer #1 certificate SHA-256 digest: ${fs.readFileSync('apps/android/signing-certificate.sha256','utf8').trim()}`))throw Error('APK certificate is incompatible with installed app');
const info=execFileSync(`${process.env.ANDROID_HOME}/build-tools/35.0.0/aapt`,['dump','badging',input],{encoding:'utf8'});
if(!info.includes(`package: name='app.farsail.android' versionCode='${version.versionCode}' versionName='${version.version}'`))throw Error('APK metadata does not match release');
if(info.includes('application-debuggable'))throw Error('Do not publish a debuggable APK');
const digest=crypto.createHash('sha256').update(bytes).digest('hex');
const payload={packageName:'app.farsail.android',version:version.version,versionCode:version.versionCode,url:`https://github.com/${repo}/releases/download/${tag}/${name}`,sha256:digest,size:bytes.length,notes:'明确显示共享状态及失败原因，定位辅助功能授权；同账号电脑无需手机重复确认。服务端需升级安卓共享支持。'};
const raw=Buffer.from(JSON.stringify(payload));
const signing=crypto.createPrivateKey(process.env.ANDROID_UPDATE_PRIVATE_KEY||fs.readFileSync('.local/android-signing/updates-private.pem'));
const publicBytes=crypto.createPublicKey(signing).export({type:'spki',format:'der'}).subarray(-32);
if(publicBytes.toString('hex')!==fs.readFileSync('crates/android/src/update-public-key.hex','utf8').trim())throw Error('Update signing key mismatch');
const envelope=Buffer.from(JSON.stringify({payload:raw.toString('base64'),signature:crypto.sign(null,raw,signing).toString('base64')})+'\n');
const publicKey=crypto.createPublicKey({key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),publicBytes]),format:'der',type:'spki'});
const verify=buffer=>{const e=JSON.parse(buffer);const p=Buffer.from(e.payload,'base64');if(!crypto.verify(null,p,publicKey,Buffer.from(e.signature,'base64')))throw Error('Untrusted previous Android channel');return JSON.parse(p)};
const dir='.local/android-publication';fs.mkdirSync(dir,{recursive:true});
fs.writeFileSync(`${dir}/${name}`,bytes);fs.writeFileSync(`${dir}/${name}.sha256`,`${digest}  ${name}\n`);fs.writeFileSync(`${dir}/latest.json`,envelope);
fs.writeFileSync(`${dir}/verification.json`,JSON.stringify({source,version,...payload,certificateSha256:fs.readFileSync('apps/android/signing-certificate.sha256','utf8').trim()},null,2)+'\n');
const getRelease=t=>{try{return api(`releases/tags/${t}`)}catch(e){if(String(e.stderr).includes('404'))return null;throw e}};
const readPublic=async url=>{for(let attempt=0;attempt<6;attempt++){const response=await fetch(url);if(response.ok)return Buffer.from(await response.arrayBuffer());if(attempt===5)throw Error(`Public download failed: ${response.status}`);await new Promise(resolve=>setTimeout(resolve,5000));}};
let channel=getRelease('android-updates');const old=channel?.assets.find(a=>a.name==='latest.json');let previous;
if(old){previous=await readPublic(old.browser_download_url);const current=verify(previous);if(current.versionCode>version.versionCode)throw Error('Android channel cannot go backwards');if(current.versionCode===version.versionCode){if(previous.equals(envelope)){console.log('Exact Android release already published');process.exit(0)}throw Error('Cannot replace published Android version with different bytes')}}
try{let ref=api(`git/ref/tags/${tag}`).object;while(ref.type==='tag')ref=api(`git/tags/${ref.sha}`).object;if(ref.sha!==source)throw Error('Existing Android tag refers to another source')}catch(e){if(!String(e.stderr).includes('404'))throw e}
let release=getRelease(tag);
if(!release){fs.writeFileSync(`${dir}/release-notes.txt`,`Native Android host with independent account/settings pages and authenticated updates. Built from ${source}. Installation requires normal Android system confirmation. Server must include Android host capability support.`);gh('release','create',tag,'-R',repo,'--target',source,'--prerelease','--title',`FarSail Android ${version.version}`,'--notes-file',`${dir}/release-notes.txt`);release=getRelease(tag)}
for(const filename of [name,`${name}.sha256`,'latest.json','verification.json']){
 const file=`${dir}/${filename}`,data=fs.readFileSync(file);let asset=release.assets.find(a=>a.name===filename);
 if(!asset){gh('release','upload',tag,file,'-R',repo);release=getRelease(tag);asset=release.assets.find(a=>a.name===filename)}
 if(!(await readPublic(asset.browser_download_url)).equals(data))throw Error(`Published Android bytes differ: ${filename}`);
}
if(!channel){gh('release','create','android-updates','-R',repo,'--target',source,'--prerelease','--title','FarSail Android update channel','--notes','Signed Android update metadata. The app validates the signature, package identity, version and installed certificate.');channel=getRelease('android-updates')}
const staged=`latest-${process.env.GITHUB_RUN_ID||Date.now()}.json`;fs.writeFileSync(`${dir}/${staged}`,envelope);gh('release','upload','android-updates',`${dir}/${staged}`,'-R',repo);channel=getRelease('android-updates');const pending=channel.assets.find(a=>a.name===staged);
if(!(await readPublic(pending.browser_download_url)).equals(envelope))throw Error('Staged Android metadata differs');
if(old)gh('api',`repos/${repo}/releases/assets/${old.id}`,'-X','DELETE');
try{api(`releases/assets/${pending.id}`,'-X','PATCH','-f','name=latest.json')}
catch(e){if(previous){fs.writeFileSync(`${dir}/latest.json`,previous);gh('release','upload','android-updates',`${dir}/latest.json`,'-R',repo)}throw e}
if(!(await readPublic(`https://github.com/${repo}/releases/download/android-updates/latest.json`)).equals(envelope))throw Error('Final Android channel differs');
console.log(`Published ${tag}; verified APK and Android feed. Desktop update channel unchanged.`);
