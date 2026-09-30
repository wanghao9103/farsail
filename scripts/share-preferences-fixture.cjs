// Synthetic loopback account/device server; no real users, desktop frames or input.
const http = require('node:http');
const fs = require('node:fs');
const owner = '00000000-0000-0000-0000-000000000001';
const device = '00000000-0000-0000-0000-000000000002';
const session = '00000000-0000-0000-0000-000000000003';
let bound = false, sharing = false, deviceName = 'Synthetic sharing fixture';
const login = {access_token:'synthetic-access',refresh_token:'synthetic-refresh',session_id:session,access_expires_in:300};
const server = http.createServer(async (req, res) => {
  const chunks = []; for await (const chunk of req) chunks.push(chunk);
  const body = chunks.length ? JSON.parse(Buffer.concat(chunks)) : {};
  let value = null;
  switch (req.url.split('?')[0]) {
    case '/fixture/status': value={sharing,bound}; break;
    case '/v1/auth/login': case '/v1/auth/refresh': value = login; break;
    case '/v1/me': value = {id:owner,email:'fixture@example.invalid',role:'user',verified:true}; break;
    case '/v1/devices/challenge': value = {challenge_id:device,message:'synthetic fixture challenge'}; break;
    case '/v1/devices/bind': bound = true; deviceName=body.name; value={id:device,device_token:'synthetic-device'}; break;
    case '/v1/devices/heartbeat': value={generation:1}; break;
    case '/v1/devices/capability': sharing=body.can_host; value={can_host:sharing}; break;
    case '/v1/devices': value=bound?[{id:device,name:deviceName,platform:'windows',enabled:true,online:true,can_host:sharing,can_files:false,last_seen_at:'2026-09-30T00:00:00Z'}]:[]; break;
    case '/v1/remote/pending': case '/v1/remote/sessions': case '/v1/auth/sessions': value=[]; break;
    case '/v1/devices/endpoint-address': case '/v1/auth/logout': break;
    default: res.statusCode=404; value={error:'fixture route missing'};
  }
  res.setHeader('Content-Type','application/json');res.end(JSON.stringify(value));
});
server.listen(0,'127.0.0.1',()=>fs.writeFileSync(process.argv[2],JSON.stringify({server:`http://127.0.0.1:${server.address().port}`,owner,device,session})));
