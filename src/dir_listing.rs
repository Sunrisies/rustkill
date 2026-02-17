use crate::ScanStatus;

use super::models::{Cli, FileEntry};
use super::utils::{human_readable_size, progress_bar_init};
use comfy_table::{Cell, ContentArrangement, Table};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use indicatif::ProgressBar;
use log::info;
use rayon::prelude::*;
use std::fs;
use std::io::stdout;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub fn calculate_dir_size(
    path: &Path,
    human_readable: bool,
    main_pb: &ProgressBar,
    parallel: bool,
) -> (u64, String) {
    // ✅ 设置当前计算的路径
    main_pb.set_message(format!("计算 {}...", path.display()));
    // 关键：用 Arc 包装，实现线程安全共享
    let pb_arc = Arc::new(main_pb.clone());

    let total = if parallel {
        // inner_calculate_parallel(path, &pb_arc, 0)
        inner_calculate_dynamic(path, &pb_arc, 0)
    } else {
        inner_calculate_serial(path, &pb_arc)
    };

    let converted = if human_readable {
        human_readable_size(total)
    } else {
        total.to_string()
    };
    (total, converted)
}
// 动态并行：根据目录复杂度决定是否并行
fn inner_calculate_dynamic(path: &Path, pb: &Arc<ProgressBar>, depth: usize) -> u64 {
    if depth > 0 && depth <= 2 {
        // 只显示前2层，避免消息刷新太频繁
        pb.set_message(format!("计算 {}...", path.display()));
    }
    match fs::read_dir(path) {
        Ok(entries) => {
            let entries_vec: Vec<_> = entries.collect();
            //根据深度决定tick频率
            let tick_freq = if depth == 0 {
                50
            } else if depth < 3 {
                100
            } else {
                200
            };
            // 收集条目并统计信息
            let items: Vec<_> = entries_vec
                .into_iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    // 批量tick
                    if i % tick_freq == 0 {
                        pb.tick();
                    }

                    let entry = e.ok()?;
                    let metadata = entry.metadata().ok()?;
                    Some((entry.path(), metadata))
                })
                .collect();

            // 动态决策：是否使用并行
            let use_parallel = should_use_parallel(&items, depth);

            if use_parallel {
                // 并行处理
                items
                    .into_par_iter()
                    .map(|(item_path, metadata)| {
                        if metadata.is_dir() {
                            inner_calculate_dynamic(&item_path, pb, depth + 1)
                        } else {
                            metadata.len()
                        }
                    })
                    .sum()
            } else {
                // 串行处理
                let mut total = 0;
                for (item_path, metadata) in items {
                    if metadata.is_dir() {
                        total += inner_calculate_dynamic(&item_path, pb, depth + 1);
                    } else {
                        total += metadata.len();
                    }
                }
                total
            }
        }
        Err(e) => {
            eprintln!("无法读取目录 {}: {}", path.display(), e);
            0
        }
    }
}
// 智能决策：是否使用并行
fn should_use_parallel(items: &[(PathBuf, std::fs::Metadata)], depth: usize) -> bool {
    // 如果深度太大，直接返回false
    if depth > 10 {
        return false;
    }

    // 统计子目录数量
    let dir_count = items.iter().filter(|(_, m)| m.is_dir()).count();
    // 策略1：根据子目录数量决定
    //子目录越多，越应该并行
    if dir_count > 8 {
        return true;
    }

    // 策略2：根据总项数决定
    // 项数越多，越应该并行
    if items.len() > 100 {
        return true;
    }

    // 策略3：根据深度调整
    // 深度越大，越应该谨慎并行
    if depth > 5 {
        return dir_count > 4; // 只有子目录多才并行
    }

    // 策略4：混合模式
    // 浅层大胆并行，深层保守
    depth < 3 || (depth < 6 && dir_count > 2)
}

// 串行版本：用于深度过大或小目录
fn inner_calculate_serial(path: &Path, pb: &Arc<ProgressBar>) -> u64 {
    pb.set_message(format!("计算 {}...", path.display()));
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            pb.tick();
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    total += inner_calculate_serial(&entry.path(), pb);
                } else {
                    total += metadata.len();
                }
            }
        }
    }
    total
}

pub fn list_directory(path: &Path, args: &Cli) -> Vec<FileEntry> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("ls: cannot access '{}': {}", path.display(), e);
            return Vec::new();
        }
    };
    let mut files: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        files.push(file_name);
    }

    files.sort();
    let scan_pb = progress_bar_init(None).unwrap();

    let mut entries = Vec::new(); // 新增存储条目信息的结构

    let process_pb = progress_bar_init(None).unwrap(); // 修改为不传入具体数值
    process_pb.set_message("处理中..."); // 设置固定提示信息
    let pb_arc = Arc::new(&process_pb);
    for (_i, file) in files.iter().enumerate() {
        process_pb.tick();
        let file_path = path.join(&file);
        let name = String::from("node_modules");
        // if name {
        let metadata = match file_path.metadata() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("ls: cannot access '{}': {}", file_path.display(), e);
                continue;
            }
        };
        if metadata.is_dir() {
            // 如果是目录，是否跟要搜索的名称匹配
            if !file.contains(&name) {
                // 使用并行版本
                calculate_dir_size_parallel(
                    file_path,
                    true,
                    Arc::clone(&pb_arc), // 克隆 Arc
                    &name,
                    &mut entries,
                );
                // info!("其他的");
                continue;
            }
        } else {
            continue;
        }
        // }
        let metadata = match file_path.metadata() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("ls: cannot access '{}': {}", file_path.display(), e);
                continue;
            }
        };

        let (size_display, size_raw) = (human_readable_size(metadata.len()), metadata.len());
        let entry = FileEntry {
            file_type: if metadata.is_dir() { 'd' } else { '-' },
            permissions: format!(
                "{}-{}-{}",
                if metadata.permissions().readonly() {
                    "r"
                } else {
                    " "
                },
                "w",
                "x"
            ),
            size_display,
            size_raw,
            path: match file_path.canonicalize() {
                Ok(canonical_path) => get_canonical_path(&canonical_path),
                Err(_e) => {
                    // eprintln!("获取绝对路径失败: {}", e);
                    file_path.to_string_lossy().into_owned()
                }
            },
        };
        // info!("添加条目: {:?}", entry);
        entries.push(entry);
    }

    process_pb.finish_and_clear();
    let mut sum_size = 0;
    for entry in &entries {
        sum_size += entry.size_raw; // 使用第4个字段的原始大小
    }

    let mut table = Table::new();
    table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("类型").add_attribute(comfy_table::Attribute::Bold),
            Cell::new("权限").add_attribute(comfy_table::Attribute::Bold),
            Cell::new("大小").add_attribute(comfy_table::Attribute::Bold),
            Cell::new("路径").add_attribute(comfy_table::Attribute::Bold),
        ])
        .load_preset(comfy_table::presets::UTF8_FULL)
        .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS);

    for entry in entries.iter() {
        let file_path = &entry.path;

        table.add_row(vec![
            Cell::new(&entry.file_type.to_string())
                .set_alignment(comfy_table::CellAlignment::Center),
            Cell::new(entry.permissions.replace('-', "")),
            Cell::new(&entry.size_display),
            Cell::new(file_path),
        ]);
    }

    println!("{}", table);
    println!("┌{:─^33}┐", "");
    println!(
        "│ 总数量: {:6} │ 总大小: {:10} ",
        entries.len(),
        human_readable_size(sum_size)
    );
    println!("└{:─^33}┘", "");

    scan_pb.finish_and_clear(); // 完成后清理进度条
    entries // 返回收集到的条目
}

/// 交互式搜索函数，使用通道实时返回结果
pub fn search_directory_interactive(path: &Path, pattern: &str) -> Receiver<FileEntry> {
    let (tx, rx) = channel();
    let pattern = pattern.to_string();
    let path = path.to_path_buf();

    thread::spawn(move || {
        search_directory_recursive(&path, &pattern, &tx);
    });

    rx
}

/// 交互式搜索并显示结果，支持按键控制
pub fn search_and_display_interactive(path: &Path, pattern: &str) -> Result<(), anyhow::Error> {
    // 进入终端原始模式
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;

    // 获取搜索结果通道
    let rx = search_directory_interactive(path, pattern);

    // 显示提示信息
    println!("搜索模式: {}", pattern);
    println!("按 Enter 继续显示下一页，按 q 退出");
    println!("按 Ctrl+C 强制退出");
    println!();

    let mut page_size = 20; // 每页显示20条
    let mut current_page = 0;
    let mut entries = Vec::new();

    // 收集所有结果（为了分页显示）
    for entry in rx {
        entries.push(entry);
    }

    // 分页显示
    let total_pages = (entries.len() + page_size - 1) / page_size;

    loop {
        // 清屏并显示当前页
        execute!(
            stdout(),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
        )?;
        execute!(stdout(), crossterm::cursor::MoveTo(0, 0))?;

        println!("搜索模式: {}", pattern);
        println!("按 Enter 继续显示下一页，按 q 退出");
        println!("按 Ctrl+C 强制退出");
        println!(
            "结果总数: {} | 当前页: {}/{}",
            entries.len(),
            current_page + 1,
            total_pages
        );
        println!();

        // 显示当前页的条目
        let start = current_page * page_size;
        let end = std::cmp::min(start + page_size, entries.len());

        if start >= entries.len() {
            println!("已显示所有结果");
        } else {
            for entry in &entries[start..end] {
                println!(
                    "{} {} {} {}",
                    entry.file_type, entry.permissions, entry.size_display, entry.path
                );
            }
        }

        println!();
        println!("按 Enter 继续，按 q 退出...");

        // 等待用户输入
        loop {
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key_event) = event::read()? {
                    match key_event {
                        KeyEvent {
                            code: KeyCode::Char('q'),
                            ..
                        } => {
                            // 退出
                            disable_raw_mode()?;
                            execute!(stdout(), LeaveAlternateScreen)?;
                            return Ok(());
                        }
                        KeyEvent {
                            code: KeyCode::Enter,
                            ..
                        } => {
                            // 继续下一页
                            if current_page + 1 < total_pages {
                                current_page += 1;
                            } else {
                                // 已到最后一页，退出
                                disable_raw_mode()?;
                                execute!(stdout(), LeaveAlternateScreen)?;
                                return Ok(());
                            }
                            break;
                        }
                        KeyEvent {
                            code: KeyCode::Char('c'),
                            modifiers: KeyModifiers::CONTROL,
                            ..
                        } => {
                            // Ctrl+C 退出
                            disable_raw_mode()?;
                            execute!(stdout(), LeaveAlternateScreen)?;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// 递归搜索目录，通过通道发送结果
fn search_directory_recursive(path: &Path, pattern: &str, tx: &Sender<FileEntry>) {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("无法读取目录 {}: {}", path.display(), e);
            return;
        }
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();

        // 检查是否匹配模式
        if file_name.contains(pattern) {
            let file_path = entry.path();
            let metadata = match file_path.metadata() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("无法获取元数据 {}: {}", file_path.display(), e);
                    continue;
                }
            };

            let (size_display, size_raw) = (human_readable_size(metadata.len()), metadata.len());
            let entry = FileEntry {
                file_type: if metadata.is_dir() { 'd' } else { '-' },
                permissions: format!(
                    "{}-{}-{}",
                    if metadata.permissions().readonly() {
                        "r"
                    } else {
                        " "
                    },
                    "w",
                    "x"
                ),
                size_display,
                size_raw,
                path: match file_path.canonicalize() {
                    Ok(canonical_path) => get_canonical_path(&canonical_path),
                    Err(_e) => file_path.to_string_lossy().into_owned(),
                },
            };

            // 发送结果到通道
            if tx.send(entry).is_err() {
                // 接收端已关闭
                return;
            }
        }

        // 如果是目录，递归搜索
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_dir() {
                search_directory_recursive(&entry.path(), pattern, tx);
            }
        }
    }
}

// 搜索文件
fn calculate_dir_size_parallel(
    file_path: PathBuf,
    human_readable: bool,
    pb: Arc<&ProgressBar>, // 改为 Arc
    name: &str,
    entries: &mut Vec<FileEntry>,
) {
    let sub_entries = match fs::read_dir(&file_path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("ls: cannot access '{}': {}", file_path.display(), e);
            return;
        }
    };

    // 收集所有需要处理的目录
    let dirs_to_process: Vec<_> = sub_entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                return None;
            }
            let metadata = e.metadata().ok()?;
            if !metadata.is_dir() {
                return None;
            }
            Some((e.path(), name))
        })
        .collect();

    // 并行处理每个子目录
    let results: Vec<Vec<FileEntry>> = dirs_to_process
        .into_par_iter()
        .map(|(sub_path, sub_name)| {
            pb.tick();
            let mut local_entries = Vec::new();
            if sub_name.contains(name) {
                // 匹配：计算大小
                let (raw, converted) = calculate_dir_size(&sub_path, human_readable, &pb, true);
                local_entries.push(FileEntry {
                    file_type: 'd',
                    permissions: "rwx".to_string(),
                    size_display: converted,
                    size_raw: raw,
                    path: get_canonical_path(&sub_path),
                });
                info!("子目录: {:?},name:{:?}", sub_name, name);
            // continue;
            } else {
                calculate_dir_size_parallel(
                    sub_path,
                    human_readable,
                    Arc::clone(&pb),
                    name,
                    &mut local_entries,
                );
                // info!("进入");
            }
            // info!("子目录处理完成: {:?}", local_entries);
            local_entries
        })
        .collect();

    // 收集所有结果到主entries
    for result in results {
        entries.extend(result);
    }
}

fn get_canonical_path(path: &Path) -> String {
    match path.canonicalize() {
        Ok(canonical) => {
            let s = canonical.to_string_lossy().into_owned();
            s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
        }
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

// 添加新的扫描函数，支持进度更新
pub fn scan_directory_with_progress(path: &Path, status_tx: &Sender<ScanStatus>) -> Vec<FileEntry> {
    // 发送初始状态
    let _ = status_tx.send(ScanStatus::Scanning {
        current_path: path.display().to_string(),
        progress: 0,
    });

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("ls: cannot access '{}': {}", path.display(), e);
            return Vec::new();
        }
    };

    let mut files: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        files.push(file_name);
    }

    files.sort();
    let total_files = files.len();
    let mut processed_files = 0;

    let mut entries = Vec::new();
    let name = String::from("node_modules");

    for (_i, file) in files.iter().enumerate() {
        let file_path = path.join(&file);

        // 更新进度
        processed_files += 1;
        let progress = (processed_files as f64 / total_files as f64 * 100.0) as u16;
        let _ = status_tx.send(ScanStatus::Scanning {
            current_path: file_path.display().to_string(),
            progress,
        });

        let metadata = match file_path.metadata() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("ls: cannot access '{}': {}", file_path.display(), e);
                continue;
            }
        };

        if metadata.is_dir() {
            if !file.contains(&name) {
                // 递归扫描子目录
                let sub_entries = scan_directory_with_progress(&file_path, status_tx);
                entries.extend(sub_entries);
                continue;
            }
        } else {
            // 处理文件
            let (size_display, size_raw) = (human_readable_size(metadata.len()), metadata.len());
            let entry = FileEntry {
                file_type: if metadata.is_dir() { 'd' } else { '-' },
                permissions: format!(
                    "{}-{}-{}",
                    if metadata.permissions().readonly() {
                        "r"
                    } else {
                        " "
                    },
                    "w",
                    "x"
                ),
                size_display,
                size_raw,
                path: match file_path.canonicalize() {
                    Ok(canonical_path) => get_canonical_path(&canonical_path),
                    Err(_e) => file_path.to_string_lossy().into_owned(),
                },
            };
            info!("添加条目: {:?}", entry);
            entries.push(entry);
        }
    }

    // 计算总大小
    let total_size: u64 = entries.iter().map(|e| e.size_raw).sum();

    // 发送完成状态
    let _ = status_tx.send(ScanStatus::Completed {
        total_files: entries.len(),
        total_size: human_readable_size(total_size),
    });

    entries
}
