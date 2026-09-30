(async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke;
  try {
    if (mode === 'enable') {
      await invoke('set_server', {server});
      await invoke('call', {op:'login',args:{email:'fixture@example.invalid',password:'synthetic-password'}});
      await invoke('call', {op:'bind',args:{name:'Synthetic sharing fixture'}});
      await invoke('transport_start', {relayUrl:null,forceRelay:false,bindAddr:'0.0.0.0:0'});
      await invoke('share_enable');
      await invoke('remote_watch', {enabled:true});
    } else {
      for (let i=0;i<60;i++) {
        const state=await invoke('state');
        if (state.sharePreferences.restore !== 'pending') break;
        await new Promise(resolve=>setTimeout(resolve,500));
      }
      if (mode === 'watch-off') await invoke('remote_watch',{enabled:false});
      if (mode === 'share-off') await invoke('share_disable');
    }
    const state=await invoke('state');
    await invoke('ipc_smoke_report',{result:JSON.stringify({ok:true,mode,state})});
    await invoke('plugin:window|close');
  } catch(e) {
    await invoke('ipc_smoke_report',{result:JSON.stringify({ok:false,mode,error:String(e)})});
  }
})();
