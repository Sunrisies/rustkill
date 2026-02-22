pub mod dir_listing;
pub mod logger;
pub mod models;
pub mod utils;
pub use dir_listing::{list_directory, scan_directory_with_progress};

use clap::Parser;
use logger::init_logger;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};
use std::{fs, thread};

use crossterm::event::{self, KeyCode, KeyEventKind};
use models::Cli;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::models::{DeleteStatus, FileEntry};
use crate::utils::human_readable_size;

fn main() -> Result<(), anyhow::Error> {
    init_logger();

    // 存储扫描结果
    let args = Cli::parse();
    let path = Path::new(&args.dir);

    // 检查是否启用了交互式搜索模式
    if path.is_dir() {
        // 使用TUI显示结果
        match scan_directory_with_ui(path) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("扫描失败: {}", e);
            }
        }
    } else {
        println!("{}", path.display());
    }
    Ok(())
}
// 定义扫描状态
#[derive(Debug, Clone)]
pub enum ScanStatus {
    /// 扫描中
    Scanning {
        current_path: String,
        progress: u16,
        total_items: usize,
        processed_items: usize,
    },
    /// 扫描完成
    Completed {
        total_files: usize,
        total_size: String,
    },
}
// 扫描目录并显示进度
fn scan_directory_with_ui(path: &Path) -> color_eyre::Result<Vec<FileEntry>> {
    let (status_tx, status_rx) = mpsc::channel::<ScanStatus>();
    let (result_tx, result_rx) = mpsc::channel::<FileEntry>();

    // 在后台线程中执行扫描
    let path_clone = path.to_path_buf();
    thread::spawn(move || {
        // 调用实际的扫描函数
        scan_directory_with_progress(&path_clone, &status_tx, &result_tx);
    });

    // 运行TUI界面显示扫描进度
    let entries = run_scan_ui(status_rx, result_rx)?;

    Ok(entries)
}
// 运行扫描UI
fn run_scan_ui(
    status_rx: Receiver<ScanStatus>,
    entries_rx: Receiver<FileEntry>,
) -> color_eyre::Result<Vec<FileEntry>> {
    color_eyre::install()?;

    let mut current_status = ScanStatus::Scanning {
        current_path: "初始化扫描...".to_string(),
        progress: 0,
        total_items: 0,
        processed_items: 0,
    };

    // 存储扫描结果
    let mut entries = Vec::new();
    let mut list_state = ListState::default().with_selected(Some(0));

    // 动画帧计数器
    let mut frame_count = 0;
    let start_time = Instant::now();
    let mut last_update_time = Instant::now();
    let update_interval = Duration::from_millis(100); // 每100ms更新一次
    let poll_timeout = Duration::from_millis(10); // 事件轮询超时时间

    ratatui::run(|terminal| loop {
        // 检查是否有新的状态更新
        let previous_status = current_status.clone();
        if let Ok(status) = status_rx.try_recv() {
            current_status = status.clone();
            log::info!("接收数据:{:?}", status);

            // 如果状态从扫描变为完成，立即更新UI
            if matches!(previous_status, ScanStatus::Scanning { .. })
                && matches!(current_status, ScanStatus::Completed { .. })
            {
                terminal.draw(|frame| {
                    render_scan_ui(
                        frame,
                        &current_status,
                        frame_count,
                        start_time,
                        &entries,
                        &mut list_state,
                    );
                })?;
            }
        }

        // 检查是否有新的条目
        let mut has_new_entries = false;
        while let Ok(entry) = entries_rx.try_recv() {
            entries.push(entry);
            has_new_entries = true;
        }

        // 如果有新条目且状态是扫描中，立即更新UI
        if has_new_entries && matches!(current_status, ScanStatus::Scanning { .. }) {
            terminal.draw(|frame| {
                render_scan_ui(
                    frame,
                    &current_status,
                    frame_count,
                    start_time,
                    &entries,
                    &mut list_state,
                );
            })?;
        }

        // 根据状态决定是否需要定期更新UI
        let needs_periodic_update = matches!(current_status, ScanStatus::Scanning { .. });

        let now = Instant::now();
        // 渲染UI
        if needs_periodic_update && now.duration_since(last_update_time) >= update_interval {
            last_update_time = now;
            frame_count += 1;

            // 渲染UI
            terminal.draw(|frame| {
                render_scan_ui(
                    frame,
                    &current_status,
                    frame_count,
                    start_time,
                    &entries,
                    &mut list_state,
                );
            })?;
        }

        // 使用poll而不是read来检查按键事件，避免阻塞
        if event::poll(poll_timeout)? {
            if let event::Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let mut needs_render = false;
                    let mut delete_success = false;
                    match key.code {
                        KeyCode::Char('j') | KeyCode::Down => {
                            // 检查是否有条目
                            if !entries.is_empty() {
                                list_state.select_next();
                                needs_render = true;
                                // 确保选中索引有效
                                // if let Some(selected) = list_state.selected() {
                                //     if selected >= entries.len() {
                                //         list_state.select(Some(entries.len() - 1));
                                //     }
                                //     log::info!("选中项: {:?}", entries[selected].path);
                                // }
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            // 检查是否有条目
                            if !entries.is_empty() {
                                list_state.select_previous();
                                needs_render = true;
                                // 确保选中索引有效
                                if let Some(selected) = list_state.selected() {
                                    log::info!("选中项: {:?}", entries[selected].path);
                                }
                            }
                        }
                        KeyCode::Char(' ') => {
                            // 空格键删除选中项
                            if let Some(selected) = list_state.selected() {
                                if selected < entries.len() {
                                    let entry = &mut entries[selected];
                                    log::info!("删除选中项: {:?}", entry);
                                    // 根据删除状态执行不同操作
                                    match entry.delete_status {
                                        DeleteStatus::NotDeleted => {
                                            // 未删除，执行删除操作
                                            entry.delete_status = DeleteStatus::Deleting;
                                            needs_render = true;
                                            log::info!("开始删除: {:?}", entry.path);
                                            // 实际执行删除操作
                                            match fs::remove_dir_all(&entry.path) {
                                                Ok(_) => {
                                                    // 删除成功，标记为已删除
                                                    entry.delete_status = DeleteStatus::Deleted;
                                                    needs_render = true;
                                                }
                                                Err(e) => {
                                                    // 删除失败，恢复为未删除状态
                                                    entry.delete_status = DeleteStatus::NotDeleted;
                                                    // delete_error = Some(format!("删除失败: {}", e));
                                                    needs_render = true;
                                                }
                                            }
                                        }
                                        DeleteStatus::Deleting => {
                                            entry.delete_status = DeleteStatus::Deleting;

                                            // 删除中，不做任何操作
                                            log::info!("条目正在删除中: {:?}", entry.path);
                                        }
                                        DeleteStatus::Deleted => {
                                            // 已删除，恢复
                                            log::info!("这个已经删除过了: {:?}", entry.path);
                                            needs_render = true;
                                        }
                                    }
                                    // needs_render = true;
                                    // // 检查是否已经删除
                                    // if entry.deleted {
                                    //     // 如果已经删除，则恢复
                                    //     log::info!("恢复已删除项: {:?}", entry.path);
                                    //     entry.deleted = false;
                                    //     needs_render = true;
                                    // } else {
                                    //     // 实际执行删除操作
                                    //     match fs::remove_dir_all(&entry.path) {
                                    //         Ok(_) => {
                                    //             // 删除成功，标记为已删除
                                    //             entry.deleted = true;
                                    //             needs_render = true;
                                    //         }
                                    //         Err(e) => {
                                    //             // 删除失败，记录错误
                                    //             needs_render = true;
                                    //         }
                                    //     }
                                    // }
                                }
                            }
                        }
                        KeyCode::Char('q') | KeyCode::Esc => break Ok(entries),
                        _ => {}
                    }
                    // 如果需要渲染，立即更新UI
                    if needs_render {
                        terminal.draw(|frame| {
                            render_scan_ui(
                                frame,
                                &current_status,
                                frame_count,
                                start_time,
                                &entries,
                                &mut list_state,
                            );
                        })?;
                    }
                }
            }
        }
    })
}

// 渲染扫描UI
fn render_scan_ui(
    frame: &mut Frame,
    status: &ScanStatus,
    frame_count: u64,
    start_time: Instant,
    entries: &[FileEntry],
    list_state: &mut ListState,
) {
    // 计算总大小
    let total_size: u64 = entries.iter().map(|e| e.size_raw).sum();
    let releasable_space = human_readable_size(total_size);
    let space_saved = "0.00 GB".to_string();
    let elapsed = start_time.elapsed();
    let search_time = format!("{:.2}s", elapsed.as_secs_f64());

    // 主布局：上（头部）、中（列表区域）、下（底部栏）
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(9), // 头部：Logo + 统计
            Constraint::Fill(1),   // 中部：扫描进度 或 结果列表
            Constraint::Length(2), // 底部：表头 + 操作提示
        ])
        .split(frame.area());

    // ========== 头部区域（不变）==========
    let header_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(42), Constraint::Min(30)])
        .split(main_layout[0]);

    let logo = Paragraph::new(
        r#"
    ____             __ __ _      __
   / __ \__  _______/ //_/(_)____/ /_
  / /_/ / / / / ___/ ,<  / / ___/ __ \
 / _, _/ /_/ (__  ) /| |/ (__  ) / / /
/_/ |_|\__,_/____/_/ |_/_/____/_/ /_/
                                     0.1.0"#,
    )
    .style(Style::default().fg(Color::Cyan));
    frame.render_widget(logo, header_layout[0]);

    let info_lines = vec![
        Line::from(vec![
            Span::styled("Releasable space: ", Style::default().fg(Color::Gray)),
            Span::styled(releasable_space, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Space saved: ", Style::default().fg(Color::Gray)),
            Span::styled(space_saved, Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Search completed ", Style::default().fg(Color::Green)),
            Span::styled(search_time, Style::default().fg(Color::Cyan)),
        ]),
    ];
    let info = Paragraph::new(Text::from(info_lines));
    frame.render_widget(info, header_layout[1]);

    // ========== 中部区域 ==========
    match status {
        ScanStatus::Scanning {
            current_path,
            progress,
            total_items,
            processed_items,
        } => {
            // 扫描中：显示进度
            let scan_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Fill(1),
                ])
                .split(main_layout[1]);

            let spinner_chars = ['-', '\\', '|', '/'];
            let spinner_index = (frame_count / 2) as usize % spinner_chars.len();
            let duration_str = format!(
                "{:02}:{:02}",
                elapsed.as_secs() / 60,
                elapsed.as_secs() % 60
            );

            let animation_text = Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("{} ", spinner_chars[spinner_index]),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled("扫描中...", Style::default().fg(Color::White)),
                Span::styled(
                    format!(" [{}]", duration_str),
                    Style::default().fg(Color::Gray),
                ),
            ]))
            .block(Block::default().borders(Borders::ALL).title("状态"))
            .alignment(Alignment::Center);
            frame.render_widget(animation_text, scan_layout[0]);

            let progress_bar = Paragraph::new(Line::from(vec![
                Span::raw("进度: "),
                Span::styled(format!("{}%", progress), Style::default().fg(Color::Green)),
                Span::raw(format!(" ({}/{})", processed_items, total_items)),
            ]))
            .block(Block::default().borders(Borders::ALL).title("扫描进度"))
            .alignment(Alignment::Center);
            frame.render_widget(progress_bar, scan_layout[1]);

            let path_text = Paragraph::new(current_path.as_str())
                .block(Block::default().borders(Borders::ALL).title("当前路径"))
                .wrap(Wrap { trim: true });
            frame.render_widget(path_text, scan_layout[2]);
        }
        ScanStatus::Completed { .. } => {
            // 扫描完成：显示可操作的列表
            let list_area = main_layout[1];

            // 列宽定义（与底部表头对齐）
            let path_width = list_area.width.saturating_sub(30); // 剩余空间给 Path
            let last_mod_width = 10;
            let size_width = 12;

            let items: Vec<ListItem> = entries
                .iter()
                .enumerate()
                .map(|(_i, e)| {
                    log::info!("删除{:?}", e);
                    let path_display = if e.path.len() > path_width as usize {
                        format!("...{}", &e.path[e.path.len() - path_width as usize + 3..])
                    } else {
                        e.path.clone()
                    };
                    // 根据删除状态添加不同的前缀
                    let status_prefix = match e.delete_status {
                        DeleteStatus::NotDeleted => Span::raw(""),
                        DeleteStatus::Deleting => {
                            Span::styled("[DELETING] ", Style::default().fg(Color::Yellow))
                        }
                        DeleteStatus::Deleted => {
                            Span::styled("[DELETED] ", Style::default().fg(Color::Green))
                        }
                    };
                    let line = Line::from(vec![
                        status_prefix,
                        Span::raw(format!(
                            "{:<width$}",
                            path_display,
                            width = path_width as usize
                        )),
                        Span::raw("  "),
                        Span::styled(
                            format!("{:>width$}", e.size_display, width = last_mod_width),
                            Style::default().fg(Color::Gray),
                        ),
                        Span::raw("  "),
                        Span::styled(
                            format!(
                                "{:>width$}",
                                human_readable_size(e.size_raw),
                                width = size_width
                            ),
                            Style::default().fg(Color::Cyan),
                        ),
                    ]);
                    ListItem::new(line)
                })
                .collect();

            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("扫描结果 ({} items)", entries.len())),
                )
                .highlight_style(Style::default().bg(Color::Yellow).fg(Color::Black));
            // .highlight_symbol(">> ");
            frame.render_stateful_widget(list, list_area, list_state);
        }
    }

    // ========== 底部栏（表头 + 操作提示）==========
    let bottom_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 表头
            Constraint::Length(1), // 操作提示
        ])
        .split(main_layout[2]);

    // 表头（与列表列宽对齐）
    let list_width = main_layout[1].width.saturating_sub(2); // 减去边框
    let path_width = list_width.saturating_sub(30);
    let last_mod_width = 10;
    let size_width = 12;

    let header_line = Line::from(vec![
        Span::styled(
            format!("{:<width$}", "Path", width = path_width as usize),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{:>width$}", "Last_mod", width = last_mod_width),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{:>width$}", "Size", width = size_width),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let header = Paragraph::new(header_line).style(Style::default().bg(Color::Rgb(60, 60, 60)));
    frame.render_widget(header, bottom_layout[0]);

    // 操作提示（橙色背景）
    let hint = Paragraph::new("CURSORS for select - SPACE to delete")
        .style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(255, 140, 0)),
        )
        .alignment(Alignment::Left);
    frame.render_widget(hint, bottom_layout[1]);
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
