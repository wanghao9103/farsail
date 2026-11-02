import fs from 'node:fs';
import crypto from 'node:crypto';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
const root = process.cwd();
const dir = path.join(root, '.local/android-signing');
const key = fs.readFileSync(path.join(dir, 'updates-private.pem'));
const publicKey = crypto.createPublicKey(crypto.createPrivateKey(key)).export({type:'spki',format:'der'}).subarray(-32).toString('hex');
if(publicKey !== fs.readFileSync('crates/android/src/update-public-key.hex','utf8').trim()) throw Error('Android update key does not match embedded public key');
const java = process.env.JAVA_HOME;
if (!java) throw Error('Set JAVA_HOME before verifying the installed APK certificate');
const certificate = spawnSync(path.join(java,'bin/keytool'),['-exportcert','-keystore',path.join(dir,'farsail-installed.jks'),'-alias','androiddebugkey','-storepass:env','FARSAIL_ANDROID_STORE_PASSWORD'],{env:{...process.env,FARSAIL_ANDROID_STORE_PASSWORD:'android'}});
if(certificate.status !== 0) throw Error('Cannot read original installed APK certificate');
if(crypto.createHash('sha256').update(certificate.stdout).digest('hex') !== fs.readFileSync('apps/android/signing-certificate.sha256','utf8').trim()) throw Error('APK signing certificate changed; installed users would not be able to update');
if(process.argv.includes('--apply')) {
  for(const [name,value] of [
    ['ANDROID_UPDATE_PRIVATE_KEY',key],
    ['ANDROID_APK_KEYSTORE_BASE64',fs.readFileSync(path.join(dir,'farsail-installed.jks')).toString('base64')],
    ['ANDROID_APK_KEYSTORE_PASSWORD','android'],
  ]) {
    const result=spawnSync('gh',['secret','set',name,'-R','wanghao9103/farsail'],{input:value,encoding:'utf8'});
    if(result.status !== 0) throw Error(`Cannot configure ${name}`);
    console.log(`Configured ${name}`);
  }
} else console.log('Android signing keys match. Pass --apply to configure repository signing secrets.');
