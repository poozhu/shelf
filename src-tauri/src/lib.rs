use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionMeta {
    session_id: String,
    file_path: String,
    cwd: String,
    title: String,
    /// 用户侧消息数(真人提问,不含工具结果/子代理)
    user_message_count: u64,
    /// 最后一条用户提问的时间(epoch 毫秒);没有提问则回退为文件修改时间
    last_active: u64,
}

/// Claude Code 把所有会话存在 ~/.claude/projects/<路径编码>/<session-id>.jsonl
fn projects_root() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude").join("projects"))
}

/// 从一条 user 记录的 content 提取纯文本;tool_result 等非真人输入返回 None
fn extract_user_text(content: &serde_json::Value) -> Option<String> {
    match content {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(blocks) => {
            if blocks
                .iter()
                .any(|b| matches!(b.get("type").and_then(|t| t.as_str()), Some("tool_result")))
            {
                return None;
            }
            let parts: Vec<&str> = blocks
                .iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
        _ => None,
    }
}

/// 压平为单行并截断,用于列表展示
fn clean_title(raw: &str) -> String {
    let line = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut title: String = line.chars().take(300).collect();
    if line.chars().count() > 300 {
        title.push('…');
    }
    title
}

/// 解析记录里的 UTC 时间戳(形如 2026-10-11T14:32:05.123)为 epoch 毫秒
fn parse_iso_ms(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 19 || b.get(10) != Some(&b'T') {
        return None;
    }
    let num = |start: usize, len: usize| -> Option<i64> { s.get(start..start + len)?.parse().ok() };
    let (year, month, day) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (hour, min, sec) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    let ms = if b.get(19) == Some(&b'.') {
        num(20, 3).unwrap_or(0)
    } else {
        0
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    // Howard Hinnant 的 days_from_civil 算法
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    let secs = ((days * 24 + hour) * 60 + min) * 60 + sec;
    Some((secs * 1000 + ms) as u64)
}

fn parse_session(path: &Path, project_dir: &str) -> Option<SessionMeta> {
    let file = File::open(path).ok()?;
    let meta = file.metadata().ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;

    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut user_message_count: u64 = 0;
    let mut last_user_ms: u64 = 0;
    let mut header_lines: u32 = 0;
    let mut cwd: Option<String> = None;
    let mut summary: Option<String> = None;
    let mut first_prompt: Option<String> = None;

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // 统计用户侧消息数(真人提问)。用字节级判断避免对每行做 JSON 解析:
        // 排除工具结果(tool_result)、子代理(isSidechain)和系统注入(isMeta)的 user 记录
        if trimmed.contains("\"type\":\"user\"")
            && !trimmed.contains("\"tool_result\"")
            && !trimmed.contains("\"isSidechain\":true")
            && !trimmed.contains("\"isMeta\":true")
        {
            user_message_count += 1;
            // 顺带从原始行里抠出 timestamp 字段,避免整行 JSON 解析
            if let Some(pos) = trimmed.find("\"timestamp\":\"") {
                let rest = &trimmed[pos + 13..];
                if rest.len() >= 24 {
                    if let Some(ms) = parse_iso_ms(&rest[..24]) {
                        last_user_ms = last_user_ms.max(ms);
                    }
                }
            }
        }

        // 头部若干行用于找 cwd 和第一条提问;之后只扫描 summary 行,
        // 避免为了列表页把几十 MB 的会话全文都做 JSON 解析
        let is_header = header_lines < 100;
        header_lines += 1;
        if !is_header && !trimmed.contains("\"summary\"") {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if cwd.is_none() {
            if let Some(c) = value.get("cwd").and_then(|c| c.as_str()) {
                cwd = Some(c.to_string());
            }
        }
        match value.get("type").and_then(|t| t.as_str()) {
            Some("summary") => {
                if summary.is_none() {
                    if let Some(s) = value.get("summary").and_then(|s| s.as_str()) {
                        let cleaned = clean_title(s);
                        if !cleaned.is_empty() {
                            summary = Some(cleaned);
                        }
                    }
                }
            }
            Some("user") if is_header && first_prompt.is_none() => {
                let sidechain = value
                    .get("isSidechain")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                let is_meta = value
                    .get("isMeta")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                if !sidechain && !is_meta {
                    if let Some(content) = value.get("message").and_then(|m| m.get("content")) {
                        if let Some(text) = extract_user_text(content) {
                            let head = text.trim_start();
                            // 跳过 /clear 之类的命令注入(<command-name> 等)和系统提示
                            if !head.starts_with('<') && !head.starts_with("Caveat:") {
                                let cleaned = clean_title(&text);
                                if !cleaned.is_empty() {
                                    first_prompt = Some(cleaned);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let title = summary
        .or(first_prompt)
        .unwrap_or_else(|| "(无标题会话)".to_string());

    Some(SessionMeta {
        session_id: path.file_stem()?.to_string_lossy().to_string(),
        file_path: path.to_string_lossy().to_string(),
        cwd: cwd.unwrap_or_else(|| project_dir.to_string()),
        title,
        user_message_count,
        last_active: if last_user_ms > 0 { last_user_ms } else { mtime },
    })
}

/// 扫描所有项目的会话,按最近修改时间从近到远排序
#[tauri::command(async)]
fn list_sessions() -> Result<Vec<SessionMeta>, String> {
    let root = projects_root().ok_or_else(|| "无法定位用户主目录".to_string())?;
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for project in fs::read_dir(&root).map_err(|e| e.to_string())?.flatten() {
        let project_path = project.path();
        if !project_path.is_dir() {
            continue;
        }
        let project_name = project.file_name().to_string_lossy().to_string();
        let Ok(files) = fs::read_dir(&project_path) else {
            continue;
        };
        for entry in files.flatten() {
            let path = entry.path();
            let is_jsonl = path
                .extension()
                .map(|e| e.eq_ignore_ascii_case("jsonl"))
                .unwrap_or(false);
            if is_jsonl && path.is_file() {
                if let Some(session) = parse_session(&path, &project_name) {
                    sessions.push(session);
                }
            }
        }
    }

    sessions.sort_by(|a, b| b.last_active.cmp(&a.last_active));
    Ok(sessions)
}

/// 删除会话文件(移入系统回收站,可恢复)
#[tauri::command(async)]
fn delete_session(path: String) -> Result<(), String> {
    let root = projects_root().ok_or_else(|| "无法定位用户主目录".to_string())?;
    let target = PathBuf::from(&path);

    let target_lower = target.to_string_lossy().to_lowercase();
    let root_lower = root.to_string_lossy().to_lowercase();
    if !target_lower.starts_with(&root_lower) {
        return Err("拒绝操作:该文件不在 Claude 会话目录内".to_string());
    }
    if !target.exists() {
        return Ok(());
    }

    trash::delete(&target).map_err(|e| format!("移入回收站失败: {e}"))
}

#[derive(Serialize)]
struct TerminalInfo {
    id: String,
    name: String,
}

fn local_appdata_path(rel: &str) -> String {
    format!(
        "{}\\{}",
        std::env::var("LOCALAPPDATA").unwrap_or_default(),
        rel
    )
}

/// 查找可执行文件:先 where.exe,失败再尝试常见安装路径
fn resolve_exe(exe: &str, fallbacks: &[String]) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        if let Ok(out) = Command::new("where.exe")
            .arg(exe)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if let Some(line) = stdout.lines().next() {
                    let path = PathBuf::from(line.trim());
                    if path.exists() {
                        return Some(path);
                    }
                }
            }
        }
        for fallback in fallbacks {
            let path = PathBuf::from(fallback);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        let _ = (exe, fallbacks);
        None
    }
}

fn wt_fallbacks() -> Vec<String> {
    vec![local_appdata_path(r"Microsoft\WindowsApps\wt.exe")]
}

fn pwsh_fallbacks() -> Vec<String> {
    vec![r"C:\Program Files\PowerShell\7\pwsh.exe".to_string()]
}

fn powershell_fallbacks() -> Vec<String> {
    vec![r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe".to_string()]
}

fn bash_fallbacks() -> Vec<String> {
    vec![
        r"C:\Program Files\Git\bin\bash.exe".to_string(),
        r"C:\Program Files (x86)\Git\bin\bash.exe".to_string(),
    ]
}

/// 检测本机可用的终端,供用户双击时选择
#[tauri::command(async)]
fn list_terminals() -> Vec<TerminalInfo> {
    let mut list = Vec::new();
    if resolve_exe("wt.exe", &wt_fallbacks()).is_some() {
        list.push(TerminalInfo {
            id: "wt".into(),
            name: "Windows Terminal".into(),
        });
    }
    list.push(TerminalInfo {
        id: "cmd".into(),
        name: "命令提示符 (cmd)".into(),
    });
    if resolve_exe("pwsh.exe", &pwsh_fallbacks()).is_some() {
        list.push(TerminalInfo {
            id: "pwsh".into(),
            name: "PowerShell 7".into(),
        });
    }
    if resolve_exe("powershell.exe", &powershell_fallbacks()).is_some() {
        list.push(TerminalInfo {
            id: "powershell".into(),
            name: "Windows PowerShell".into(),
        });
    }
    if resolve_exe("bash.exe", &bash_fallbacks()).is_some() {
        list.push(TerminalInfo {
            id: "bash".into(),
            name: "Git Bash".into(),
        });
    }
    list
}

/// Claude Code 会话内部的标记/句柄类环境变量:
/// 如果 Shelf 本身是从另一个 Claude Code 会话里启动的,会继承它们,
/// 再传给新终端里的 claude 会被误判为"子会话"(转录保存被关闭)甚至串扰到原会话。
/// 注意:只剔除会话级变量,保留用户的 ANTHROPIC_* 等全局配置。
const SCRUB_ENV: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_PID",
];

fn scrub_env(cmd: &mut std::process::Command) {
    for key in SCRUB_ENV {
        cmd.env_remove(key);
    }
}

/// 经 `start` 在新控制台里运行命令(转发用的外层 cmd 自身不显示窗口)
#[cfg(windows)]
fn spawn_in_new_console(
    title: &str,
    args: Vec<std::ffi::OsString>,
    dir: &Path,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut cmd = Command::new("cmd.exe");
    cmd.arg("/c").arg("start").arg(title).args(args);
    scrub_env(&mut cmd);
    cmd.current_dir(dir)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开终端失败:{e}"))
}

/// 在指定终端里打开会话原目录,并用 `claude --resume` 恢复对应会话
#[tauri::command(async)]
fn resume_session(session_id: String, cwd: String, terminal: String) -> Result<(), String> {
    // 会话 ID 来自文件名,仍做白名单校验,防止命令注入
    let valid_id = !session_id.is_empty()
        && session_id.len() <= 64
        && session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !valid_id {
        return Err("非法的会话 ID".to_string());
    }

    let dir = Path::new(&cwd);
    if !dir.is_dir() {
        return Err(format!("会话目录不存在:{cwd}"));
    }

    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::process::Command;

        let title = format!("Shelf · {}", &session_id[..8.min(session_id.len())]);
        let resume_cmd = format!("claude --resume {session_id}");

        match terminal.as_str() {
            // Windows Terminal:直接拉起,-d 指定起始目录
            "wt" => {
                let exe = resolve_exe("wt.exe", &wt_fallbacks())
                    .ok_or_else(|| "未找到 Windows Terminal".to_string())?;
                let mut cmd = Command::new(exe);
                cmd.arg("-d")
                    .arg(dir)
                    .args(["cmd", "/k", "claude", "--resume"])
                    .arg(&session_id)
                    .current_dir(dir);
                scrub_env(&mut cmd);
                cmd.spawn()
                    .map(|_| ())
                    .map_err(|e| format!("打开终端失败:{e}"))?;
            }
            // 以下 shell 统一经 start 拉起,新进程拥有真实终端句柄
            "cmd" => {
                let mut args: Vec<OsString> = ["cmd", "/k", "claude", "--resume"]
                    .iter()
                    .copied()
                    .map(OsString::from)
                    .collect();
                args.push(session_id.into());
                spawn_in_new_console(&title, args, dir)?;
            }
            "pwsh" => {
                let exe = resolve_exe("pwsh.exe", &pwsh_fallbacks())
                    .ok_or_else(|| "未找到 PowerShell 7".to_string())?;
                spawn_in_new_console(
                    &title,
                    vec![
                        exe.into_os_string(),
                        "-NoExit".into(),
                        "-Command".into(),
                        resume_cmd.into(),
                    ],
                    dir,
                )?;
            }
            "powershell" => {
                let exe = resolve_exe("powershell.exe", &powershell_fallbacks())
                    .ok_or_else(|| "未找到 Windows PowerShell".to_string())?;
                spawn_in_new_console(
                    &title,
                    vec![
                        exe.into_os_string(),
                        "-NoExit".into(),
                        "-Command".into(),
                        resume_cmd.into(),
                    ],
                    dir,
                )?;
            }
            "bash" => {
                let exe = resolve_exe("bash.exe", &bash_fallbacks())
                    .ok_or_else(|| "未找到 Git Bash".to_string())?;
                spawn_in_new_console(&title, vec![exe.into_os_string(), "-lc".into(), resume_cmd.into()], dir)?;
            }
            other => return Err(format!("不支持的终端类型:{other}")),
        }
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_sessions,
            delete_session,
            resume_session,
            list_terminals
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
