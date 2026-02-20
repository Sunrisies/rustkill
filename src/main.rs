pub mod dir_listing;
pub mod logger;
pub mod models;
pub mod utils;
pub use dir_listing::{list_directory, scan_directory_with_progress};

use clap::Parser;
use logger::init_logger;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, KeyCode, KeyEventKind};
use models::Cli;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::models::FileEntry;
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
        if let Ok(status) = status_rx.try_recv() {
            current_status = status.clone();
            log::info!("接收数据:{:?}", status);
        }

        // 检查是否有新的条目
        while let Ok(entry) = entries_rx.try_recv() {
            entries.push(entry);
        }

        let now = Instant::now();
        // 渲染UI
        if now.duration_since(last_update_time) >= update_interval {
            last_update_time = now;
            frame_count += 1;

            // 渲染UI
            terminal.draw(|frame| {
                render_scan_ui(frame, &current_status, frame_count, start_time, &entries);
                render_results(frame, &mut list_state, &entries);
            })?;
        }

        // 使用poll而不是read来检查按键事件，避免阻塞
        if event::poll(poll_timeout)? {
            if let event::Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('j') | KeyCode::Down => list_state.select_next(),
                        KeyCode::Char('k') | KeyCode::Up => list_state.select_previous(),
                        KeyCode::Char(' ') => {
                            // 空格键删除选中项
                            if let Some(selected) = list_state.selected() {
                                if selected < entries.len() {
                                    log::info!("删除选中项: {:?}", entries[selected].path);
                                    // 这里可以添加实际的删除逻辑
                                }
                            }
                        }
                        KeyCode::Char('q') | KeyCode::Esc => break Ok(entries),
                        _ => {}
                    }
                }
            }
        }
    })
}

// 渲染扫描UI
// 渲染扫描UI
fn render_scan_ui(
    frame: &mut Frame,
    status: &ScanStatus,
    frame_count: u64,
    start_time: Instant,
    entries: &[FileEntry],
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3), // 标题
                Constraint::Length(1), // 动态数据
                Constraint::Length(3), // 动画
                Constraint::Length(3), // 进度条
                Constraint::Length(3), // 当前状态
                Constraint::Fill(1),   // 填充剩余空间
            ]
            .as_ref(),
        )
        .split(frame.area());

    // 标题
    let title = Paragraph::new("RustKill")
        .block(Block::default().borders(Borders::ALL))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // 计算总大小
    let total_size: u64 = entries.iter().map(|e| e.size_raw).sum();
    let total_size_display = human_readable_size(total_size);

    // 计算可释放空间（这里假设所有找到的node_modules都是可释放的）
    let releasable_space = total_size_display.clone();
    let space_saved = "0.00 GB".to_string();

    // 计算搜索时间
    let elapsed = start_time.elapsed();
    let search_time = format!("{:.2}s", elapsed.as_secs_f64());

    // 顶部信息行
    let top_info = Line::from(vec![
        Span::styled("可释放空间: ", Style::default().fg(Color::White)),
        Span::styled(releasable_space, Style::default().fg(Color::Green)),
        Span::styled("  节省空间: ", Style::default().fg(Color::White)),
        Span::styled(space_saved, Style::default().fg(Color::Green)),
        Span::styled("  搜索时间: ", Style::default().fg(Color::White)),
        Span::styled(search_time, Style::default().fg(Color::Cyan)),
    ]);
    frame.render_widget(Paragraph::new(top_info), chunks[1]);

    // 旋转动画
    let spinner_chars = ['-', '\\', '|', '/'];
    let spinner_index = (frame_count / 2) as usize % spinner_chars.len();
    let spinner_char = spinner_chars[spinner_index];

    let elapsed = start_time.elapsed();
    let duration_str = format!(
        "{:02}:{:02}",
        elapsed.as_secs() / 60,
        elapsed.as_secs() % 60
    );

    let animation_text = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("{} ", spinner_char),
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
    frame.render_widget(animation_text, chunks[2]);

    // 根据状态渲染不同内容
    match status {
        ScanStatus::Scanning {
            current_path,
            progress,
            total_items,
            processed_items,
        } => {
            // 进度条
            let progress_bar = Paragraph::new(Line::from(vec![
                Span::raw("进度: "),
                Span::styled(format!("{}%", progress), Style::default().fg(Color::Green)),
                Span::raw(format!(" ({}/{})", processed_items, total_items)),
            ]))
            .block(Block::default().borders(Borders::ALL).title("扫描进度"))
            .alignment(Alignment::Center);
            frame.render_widget(progress_bar, chunks[3]);

            // 当前路径
            let path_text = Paragraph::new(current_path.as_str())
                .block(Block::default().borders(Borders::ALL).title("当前路径"))
                .wrap(Wrap { trim: true });
            frame.render_widget(path_text, chunks[4]);
        }
        ScanStatus::Completed {
            total_files,
            total_size,
        } => {
            // 扫描完成时显示的提示
            let completion_text = Paragraph::new(Line::from(vec![
                Span::styled(
                    "✓",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" 扫描完成！", Style::default().fg(Color::Green)),
                Span::styled(" 按任意键继续...", Style::default().fg(Color::Gray)),
            ]))
            .block(Block::default().borders(Borders::ALL).title("完成"))
            .alignment(Alignment::Center);
            frame.render_widget(completion_text, chunks[3]);

            // 显示统计信息
            let stats_text = Paragraph::new(Line::from(vec![
                Span::styled("文件数: ", Style::default().fg(Color::White)),
                Span::styled(format!("{}", total_files), Style::default().fg(Color::Cyan)),
                Span::styled(" | 总大小: ", Style::default().fg(Color::White)),
                Span::styled(total_size.clone(), Style::default().fg(Color::Cyan)),
            ]))
            .block(Block::default().borders(Borders::ALL).title("统计"))
            .alignment(Alignment::Center);
            frame.render_widget(stats_text, chunks[4]);
        }
    }
}

fn init_render(entries: Vec<FileEntry>) -> color_eyre::Result<()> {
    let mut list_state = ListState::default().with_selected(Some(0));
    ratatui::run(|terminal| loop {
        terminal.draw(|frame| render(frame, &mut list_state, &entries))?;
        if let event::Event::Key(key) = event::read()? {
            // 仅处理按下事件
            if key.kind == event::KeyEventKind::Press {
                log::info!("Key pressed: {:?}", key.code);
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => list_state.select_next(),
                    KeyCode::Char('k') | KeyCode::Up => list_state.select_previous(),
                    KeyCode::Char(' ') => {
                        // 空格键删除选中项
                        if let Some(selected) = list_state.selected() {
                            if selected < entries.len() {
                                log::info!("删除选中项: {:?}", entries[selected].path);
                                // 这里可以添加实际的删除逻辑
                            }
                        }
                    }
                    KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                    _ => {}
                }
            }
        }
    })
}
// 渲染结果列表
fn render_results(frame: &mut Frame, list_state: &mut ListState, entries: &[FileEntry]) {
    // 操作提示行
    let hint_line = Line::from(vec![Span::styled(
        "光标选择 - 空格删除",
        Style::default().fg(Color::Yellow),
    )]);

    // 列表区域
    let constraints = [
        Constraint::Length(1), // 操作提示行
        Constraint::Fill(1),   // 列表
        Constraint::Length(1), // 版本号
    ];
    let layout = Layout::vertical(constraints).spacing(0);
    let [hint_area, list_area, version_area] = frame.area().layout(&layout);

    // 渲染操作提示行
    frame.render_widget(hint_line, hint_area);

    // 渲染列表
    render_list(frame, list_area, list_state, entries);

    // 渲染版本号
    let version_line = Line::from(vec![Span::styled(
        "0.12.2",
        Style::default().fg(Color::Gray),
    )]);
    frame.render_widget(version_line, version_area);
}

/// Render the UI with various lists.
fn render(frame: &mut Frame, list_state: &mut ListState, entries: &[FileEntry]) {
    // 计算总大小
    let total_size: u64 = entries.iter().map(|e| e.size_raw).sum();
    let total_size_display = human_readable_size(total_size);

    // 计算可释放空间（这里假设所有找到的node_modules都是可释放的）
    let releasable_space = total_size_display.clone();
    let space_saved = "0.00 GB".to_string();

    // 模拟搜索时间（实际应该从扫描过程中获取）
    let search_time = "3.82s".to_string();

    // 顶部信息行
    let top_info = Line::from(vec![
        Span::styled("可释放空间: ", Style::default().fg(Color::White)),
        Span::styled(releasable_space, Style::default().fg(Color::Green)),
        Span::styled("  节省空间: ", Style::default().fg(Color::White)),
        Span::styled(space_saved, Style::default().fg(Color::Green)),
        Span::styled("  搜索已完成 ", Style::default().fg(Color::White)),
        Span::styled(search_time, Style::default().fg(Color::Cyan)),
    ]);

    // 操作提示行
    let hint_line = Line::from(vec![Span::styled(
        "光标选择 - 空格删除",
        Style::default().fg(Color::Yellow),
    )]);

    // 列表区域
    let constraints = [
        Constraint::Length(1), // 顶部信息行
        Constraint::Length(1), // 操作提示行
        Constraint::Fill(1),   // 列表
        Constraint::Length(1), // 版本号
    ];
    let layout = Layout::vertical(constraints).spacing(0);
    let [top_area, hint_area, list_area, version_area] = frame.area().layout(&layout);

    // 渲染顶部信息行
    frame.render_widget(top_info, top_area);

    // 渲染操作提示行
    frame.render_widget(hint_line, hint_area);

    // 渲染列表
    render_list(frame, list_area, list_state, entries);

    // 渲染版本号
    let version_line = Line::from(vec![Span::styled(
        "0.1.0",
        Style::default().fg(Color::Gray),
    )]);
    frame.render_widget(version_line, version_area);
}
/// Render a list.
pub fn render_list(
    frame: &mut Frame,
    area: Rect,
    list_state: &mut ListState,
    entries: &[FileEntry],
) {
    // 计算路径列的宽度
    let max_path_len = entries.iter().map(|e| e.path.len()).max().unwrap_or(0);
    let path_width = max_path_len.min(80); // 限制最大宽度为80

    let items: Vec<Line> = entries
        .iter()
        .map(|entry| {
            // 假设每个条目都有最后修改时间（这里用占位符，实际应该从文件系统获取）
            // 在实际应用中，应该从entry中获取last_modified字段
            let last_modified = "28d"; // 占位符
            let size = &entry.size_display;
            let path = &entry.path;

            Line::from(vec![
                // 路径
                Span::styled(
                    format!("{:<width$} ", path, width = path_width),
                    Style::default().fg(Color::White),
                ),
                // 最后修改时间
                Span::styled(
                    format!("{:<8} ", last_modified),
                    Style::default().fg(Color::Green),
                ),
                // 大小
                Span::styled(format!("{:>10} ", size), Style::default().fg(Color::Cyan)),
            ])
        })
        .collect();

    let list = List::new(items)
        .style(Color::White)
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, list_state);
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
