use super::*;
use fold_platform::desktop::DesktopClient;
use std::io::Read;
pub fn run(args: &[std::ffi::OsString]) -> Result<(), String> {
    let strings = args
        .iter()
        .map(|a| a.to_str().ok_or("arguments must be UTF-8"))
        .collect::<Result<Vec<_>, _>>()?;
    match strings.as_slice(){
        ["sessions"]=>println!("{}",client::sessions()?),
        ["mcp"]=>return mcp::serve(&mut std::io::stdin().lock(),&mut std::io::stdout().lock()),
        ["call",session,method,args]=>{
            let input=if *args=="-"{let mut text=String::new();std::io::stdin().take(MAX_REQUEST as u64+1).read_to_string(&mut text).map_err(|e|e.to_string())?;text}else{args.to_string()};
            if input.len()>MAX_REQUEST{return Err("request exceeds limit".into());}
            let params=serde_json::from_str(&input).map_err(|e|e.to_string())?;
            let reply=client::call(session,method,params)?;println!("{reply}");if reply["ok"]==false{return Err("Fold request rejected; see JSON error".into());}
        }
        ["serve"]|["serve",_]=>{
            let mut session=if let Some(path)=strings.get(1){crate::session::Session::new(fold_project::load(std::path::Path::new(path),64).map_err(|e|e.to_string())?)}else{crate::session::Session::default()};
            session.set_agent_access(true);
            loop{session.poll();if let Some(e)=session.agent_access()["error"].as_str(){return Err(e.into());}std::thread::sleep(std::time::Duration::from_millis(10));}
        }
        _=>return Err("usage: fold-cli agent sessions | call SESSION METHOD JSON-or-- | mcp | serve [PROJECT.fold]".into()),
    }
    Ok(())
}
