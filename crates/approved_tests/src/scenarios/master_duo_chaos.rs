use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

pub const NAME: &str = "master_duo_chaos";
const HOST_SETUP: &str = "wait world; spawn 0; force_match_start; mark ready";
const CLIENT_SETUP: &str = "wait world; spawn 0; mark ready";
const MOVEMENT: &str = "mark movement.begin; hold +forward; hold +moveright; hold +left; wait 4s; release all; mark movement.end";
const HOST_CHAOS: &str = "move -1066 1391 7 174 85; bot tp 2 -1066 1391 127 174 85; bot tp 3 -1073 1362 80 174 85; bot tp 4 519 -44 16 61 85; bot tp 5 528 -14 16 61 85; bot tp 6 62 915 80 180 85; bot tp 7 62 900 80 180 85; bot tp 8 80 915 80 180 85; bot give 2 rpg; bot give 3 rpg; bot give 4 rpg; bot give 5 rpg; bot give 6 rpg; bot give 7 rpg; bot give 8 rpg; bot fire all; mark chaos.begin";

type Result<T> = std::result::Result<T, String>;

struct Player {
    child: Child,
    directory: PathBuf,
    role: &'static str,
}

impl Player {
    fn send(&mut self, script: &str) -> Result<()> {
        writeln!(
            self.child.stdin.as_mut().ok_or("console pipe closed")?,
            "{script}"
        )
        .map_err(|e| e.to_string())?;
        let mut log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.directory.join("commands.txt"))
            .map_err(|e| e.to_string())?;
        writeln!(log, "{script}").map_err(|e| e.to_string())
    }

    fn stdout(&self) -> Result<String> {
        fs::read_to_string(self.directory.join("stdout.log")).map_err(|e| e.to_string())
    }

    fn status(&mut self) -> Result<String> {
        if let Some(status) = self.child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "{} exited: {status}; see {}",
                self.role,
                self.directory.display()
            ));
        }
        let status = match fs::read_to_string(self.directory.join("master.status")) {
            Ok(status) => status,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.to_string()),
        };
        if ["failed", "closed", "left"].contains(&field(&status, "state").unwrap_or("")) {
            return Err(format!("{} master connection ended: {status}", self.role));
        }
        Ok(status)
    }

    fn wait(&mut self, predicate: impl Fn(&str, &str) -> bool) -> Result<String> {
        let started = Instant::now();
        loop {
            let status = self.status()?;
            let output = self.stdout()?;
            if predicate(&status, &output) {
                return Ok(status);
            }
            if started.elapsed() > Duration::from_secs(120) {
                return Err(format!(
                    "{} timed out; see {}",
                    self.role,
                    self.directory.display()
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn mark(&mut self, label: &str) -> Result<()> {
        self.wait(|_, output| event(output, label).is_some())
            .map(drop)
    }

    fn finish(&mut self) -> Result<()> {
        self.send("finish_run")?;
        let started = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().map_err(|e| e.to_string())? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("{} exit: {status}", self.role))
                };
            }
            if started.elapsed() > Duration::from_secs(30) {
                return Err(format!("{} quit timed out", self.role));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.send("quit !");
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn field<'a>(status: &'a str, key: &str) -> Option<&'a str> {
    status
        .lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(k, v)| (k == key).then_some(v))
}

fn event(output: &str, label: &str) -> Option<crate::runner::Event> {
    output
        .lines()
        .filter_map(|line| crate::runner::parse(line, "benchmark-mark:", 0))
        .find(|event| event.get("label") == Some(label))
}

fn launch(
    binary: &Path,
    cwd: &Path,
    run: &Path,
    games: &Path,
    descriptor: &Path,
    role: &'static str,
) -> Result<Player> {
    let directory = run.join(role);
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    fs::write(
        directory.join("settings.cfg"),
        format!("resolution=960x540\nfullscreen=false\nvsync=false\nplayer_name=approved-{role}\n"),
    )
    .map_err(|e| e.to_string())?;
    let mut command = Command::new(binary);
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if name.starts_with("IW4L_MASTER_") || name.starts_with("IW4L_NET_") {
            command.env_remove(key);
        }
    }
    let child = command
        .arg("menu")
        .current_dir(cwd)
        .env("IW4L_COMMUNITY", descriptor)
        .env("IW4L_GAMES", games)
        .env("IW4L_ARTIFACTS_DIR", directory.join("iw4l-artifacts"))
        .env("IW4L_SETTINGS_PATH", directory.join("settings.cfg"))
        .env("IW4L_ACCOUNT_PATH", directory.join("account.dat"))
        .env("IW4L_MASTER_STATUS_FILE", directory.join("master.status"))
        .env("IW4L_MASTER_MAX_PLAYERS", "9")
        .env("IW4L_CONSOLE_STDIN", "1")
        .env("IW4L_PERF", "1")
        .env("IW4L_PRESENT_MODE", "AutoNoVsync")
        .env_remove("IW4L_CMDS")
        .env_remove("IW4L_LOG")
        .env_remove("IW4L_TRACES_DIR")
        .stdin(Stdio::piped())
        .stdout(fs::File::create(directory.join("stdout.log")).map_err(|e| e.to_string())?)
        .stderr(fs::File::create(directory.join("stderr.log")).map_err(|e| e.to_string())?)
        .spawn()
        .map_err(|e| e.to_string())?;
    println!("{role}: pid={} cwd={}", child.id(), cwd.display());
    fs::write(directory.join("pid"), child.id().to_string()).map_err(|e| e.to_string())?;
    Ok(Player {
        child,
        directory,
        role,
    })
}

pub fn run(root: &Path, source: &Path) -> Result<bool> {
    if cfg!(windows) {
        return Err("master_duo_chaos currently requires Linux".into());
    }
    let cwd = root
        .join("context/simulated-iw4l-folder")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let descriptor = cwd.join(
        std::env::var_os("IW4L_COMMUNITY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("community-dev.iw4l-server")),
    );
    let bytes =
        fs::read_to_string(&descriptor).map_err(|e| format!("{}: {e}", descriptor.display()))?;
    let community: updater::Community = toml::from_str(&bytes).map_err(|e| e.to_string())?;
    let dev_host = community
        .master
        .address
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or("");
    if !community.master.address.ends_with(":4434")
        || (community.master.server_name != "iw4l-dev" && community.master.server_name != dev_host)
    {
        return Err("scenario requires the dev master (port 4434, TLS name iw4l-dev or the descriptor host)".into());
    }
    let games = asset_transport::games_root_from_env()?
        .0
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let source = source.canonicalize().map_err(|e| e.to_string())?;
    let binary = cwd.join("iw4l");
    let temporary = cwd.join(format!("iw4l.stage-{}", std::process::id()));
    fs::hard_link(&source, &temporary)
        .or_else(|_| fs::copy(&source, &temporary).map(drop))
        .map_err(|e| e.to_string())?;
    fs::rename(&temporary, &binary).map_err(|e| e.to_string())?;
    let run = root.join("iw4l-artifacts/approved-tests").join(format!(
        "{}-{NAME}-{}",
        crate::utc_stamp(),
        std::process::id()
    ));
    fs::create_dir_all(&run).map_err(|e| e.to_string())?;
    fs::write(run.join("community-dev.iw4l-server"), &bytes).map_err(|e| e.to_string())?;
    let mut manifest = json!({"scenario": NAME, "result": "running", "cwd": cwd,
        "binary": binary, "build": crate::report::build_facts(root, &source),
        "community": descriptor, "master": community.master.address, "map": "iw4:mp_boneyard",
        "processes": 2, "bots": 7, "cache": "cold", "gameplay_seconds": 20, "samples": []});
    if let Some((host, _)) = community.master.address.rsplit_once(':') {
        let route = Command::new("ip")
            .args(["route", "get", host])
            .output()
            .map_err(|e| e.to_string())?;
        if !route.status.success() {
            return Err("cannot observe route to dev master".into());
        }
        manifest["network_route"] = String::from_utf8_lossy(&route.stdout).trim().into();
    }
    crate::report::write(&run, &manifest)?;
    println!("run: {}", run.display());
    let outcome = execute(&binary, &cwd, &run, &games, &descriptor, &mut manifest);
    manifest["result"] = if outcome.is_ok() { "passed" } else { "failed" }.into();
    if let Err(error) = &outcome {
        manifest["failure"] = error.clone().into();
    }
    crate::report::write(&run, &manifest)?;
    println!("{}: {}", manifest["result"], run.join("run.json").display());
    Ok(outcome.is_ok())
}

fn execute(
    binary: &Path,
    cwd: &Path,
    run: &Path,
    games: &Path,
    descriptor: &Path,
    manifest: &mut Value,
) -> Result<()> {
    let mut host = launch(binary, cwd, run, games, descriptor, "host")?;
    host.send(
        "set ui_mapname iw4:mp_boneyard; set ui_gametype dm; ui_create_lobby; ui_lobby_privacy",
    )?;
    let status = host.wait(|s, _| field(s, "state") == Some("hosting"))?;
    let room = field(&status, "room").ok_or("missing room ID")?.to_owned();
    manifest["room"] = room.clone().into();
    let mut client = launch(binary, cwd, run, games, descriptor, "client")?;
    client.send(&format!(
        "set ui_mapname iw4:mp_boneyard; set ui_gametype dm; ui_join_lobby_id {room}"
    ))?;
    client.wait(|s, _| field(s, "state") == Some("joined"))?;
    host.wait(|s, _| field(s, "members") == Some("2"))?;
    host.send("ui_start_match")?;
    host.send(HOST_SETUP)?;
    client.send(CLIENT_SETUP)?;
    host.mark("ready")?;
    client.mark("ready")?;
    host.send("bot add 7; bot hold on; mark populated")?;
    host.mark("populated")?;
    for player in [&mut host, &mut client] {
        player.send("dump ready; screenshot approved/ready")?;
    }
    let started = Instant::now();
    host.send(MOVEMENT)?;
    client.send(MOVEMENT)?;
    host.mark("movement.end")?;
    client.mark("movement.end")?;
    host.send(HOST_CHAOS)?;
    client.send("hold +attack; mark chaos.begin")?;
    host.mark("chaos.begin")?;
    client.mark("chaos.begin")?;
    for index in 0..8 {
        if index == 2 {
            host.send("bot fire all")?;
        }
        let label = format!("sample.{index}");
        host.send(&format!("wait 2s; mark {label}"))?;
        client.send(&format!("wait 2s; mark {label}"))?;
        host.mark(&label)?;
        client.mark(&label)?;
        host.status()?;
        client.status()?;
        let h = event(&host.stdout()?, &label).ok_or("host sample missing")?;
        let c = event(&client.stdout()?, &label).ok_or("client sample missing")?;
        manifest["samples"]
            .as_array_mut()
            .ok_or("samples missing")?
            .push(json!({
            "elapsed_ms": started.elapsed().as_millis(), "host": h.fields, "client": c.fields,
            "tick_gap": h.num("tick").zip(c.num("tick")).map(|(h,c)| h-c)}));
        crate::report::write(run, manifest)?;
        println!(
            "sample {index}: host tick={:?}, client tick={:?}",
            h.num("tick"),
            c.num("tick")
        );
    }
    for player in [&mut host, &mut client] {
        player.send("release all; dump final; screenshot approved/final; mark final")?;
        player.mark("final")?;
    }
    for player in [&mut client, &mut host] {
        player.finish()?;
    }
    manifest["gameplay_elapsed_ms"] = json!(started.elapsed().as_millis());
    let mut assertions = Vec::new();
    for player in [&host, &client] {
        let output = player.stdout()?;
        let start = event(&output, "movement.begin").ok_or("movement begin missing")?;
        let end = event(&output, "movement.end").ok_or("movement end missing")?;
        let final_event = event(&output, "final").ok_or("final mark missing")?;
        let tick_delta = final_event
            .num("tick")
            .zip(start.num("tick"))
            .map(|(a, b)| a - b);
        assertions.push(
            json!({"name": format!("{}.simulation_advanced", player.role),
            "passed": tick_delta.is_some_and(|n| n >= 200.0), "ticks": tick_delta}),
        );
        let local_id = final_event.get("local_id");
        assertions.push(json!({"name": format!("{}.distinct_seat", player.role),
            "passed": local_id == Some(if player.role == "host" { "0" } else { "1" }), "local_id": local_id}));
        assertions.push(json!({"name": format!("{}.one_map_load", player.role),
            "passed": output.lines().filter(|line| line.contains("gsc: installed")).count() == 1}));
        let clients = final_event.num("clients");
        assertions.push(
            json!({"name": format!("{}.replicated_players", player.role),
            "passed": clients == Some(9.0), "players": clients}),
        );
        if player.role == "host" {
            for (key, threshold) in [
                ("client0_cmds", 1.0),
                ("client0_path", 32.0),
                ("client1_cmds", 1.0),
                ("client1_path", 32.0),
            ] {
                let delta = end.num(key).zip(start.num(key)).map(|(a, b)| a - b);
                assertions.push(json!({"name": key, "passed": delta.is_some_and(|n| n >= threshold), "delta": delta}));
            }
        }
        let stderr =
            fs::read_to_string(player.directory.join("stderr.log")).map_err(|e| e.to_string())?;
        assertions.push(json!({"name": format!("{}.no_script_fault", player.role),
            "passed": !stderr.contains("GSC execution failed") && !output.contains("gsc: refused")}));
    }
    for role in ["host", "client"] {
        let ticks: Vec<f64> = manifest["samples"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|sample| sample[role]["tick"].as_str()?.parse().ok())
            .collect();
        assertions.push(json!({"name": format!("{role}.continuous_replication"),
            "passed": ticks.len() == 8 && ticks.windows(2).all(|pair| pair[1] > pair[0]), "ticks": ticks}));
    }
    manifest["max_observed_tick_gap"] = manifest["samples"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|sample| sample["tick_gap"].as_f64())
        .map(f64::abs)
        .reduce(f64::max)
        .into();
    let passed = assertions.iter().all(|a| a["passed"] == true);
    manifest["assertions"] = assertions.into();
    if passed {
        Ok(())
    } else {
        Err("scenario assertions failed; see run.json".into())
    }
}
