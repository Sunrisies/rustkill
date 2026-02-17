pub mod dir_listing;
pub mod logger;
pub mod models;
pub mod utils;
pub use dir_listing::{list_directory, search_and_display_interactive};

use clap::Parser;
use logger::init_logger;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crossterm::event::{self, KeyCode, KeyEventKind};
use models::Cli;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListDirection, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::models::FileEntry;

fn main() -> Result<(), anyhow::Error> {
    init_logger();

    // 存储扫描结果
    let start_time = std::time::Instant::now();
    let args = Cli::parse();
    let path = Path::new(&args.dir);

    // 检查是否启用了交互式搜索模式
    if let Some(pattern) = &args.search {
        if path.is_dir() {
            search_and_display_interactive(path, pattern)?;
        } else {
            println!("错误: 路径不是目录: {}", path.display());
        }
    } else if path.is_dir() {
        // list_directory(path, &args);
        // entries = list_directory(path, &args);
        // 使用TUI显示结果
        match scan_directory_with_ui(path) {
            Ok(entries) => {
                // 使用TUI显示结果
                let _ = init_render(entries);
            }
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
enum ScanStatus {
    Scanning {
        current_path: String,
        progress: u16,
    },
    Completed {
        total_files: usize,
        total_size: String,
    },
}
// 扫描目录并显示进度
fn scan_directory_with_ui(path: &Path) -> color_eyre::Result<Vec<FileEntry>> {
    let (status_tx, status_rx) = mpsc::channel::<ScanStatus>();
    let (result_tx, result_rx) = mpsc::channel::<Vec<FileEntry>>();

    // 在后台线程中执行扫描
    let path_clone = path.to_path_buf();
    thread::spawn(move || {
        // 调用实际的扫描函数
        let entries = dir_listing::scan_directory_with_progress(
            &path_clone,
            &status_tx,
            Some("node_modules"),
        );
        // 扫描结束
        log::info!("扫描结束，数据是:{:?}", entries);
        // 发送结果
        let _ = result_tx.send(entries);
    });

    // 运行TUI界面显示扫描进度
    let entries = run_scan_ui(status_rx, result_rx)?;

    Ok(entries)
}
// 运行扫描UI
fn run_scan_ui(
    status_rx: Receiver<ScanStatus>,
    result_rx: Receiver<Vec<FileEntry>>,
) -> color_eyre::Result<Vec<FileEntry>> {
    let mut current_status = ScanStatus::Scanning {
        current_path: "Initializing scan...".to_string(),
        progress: 0,
    };

    ratatui::run(|terminal| loop {
        // 检查是否有新的状态更新
        if let Ok(status) = status_rx.try_recv() {
            current_status = status.clone();

            // 如果扫描完成，获取结果并退出
            if let ScanStatus::Completed { .. } = status {
                if let Ok(entries) = result_rx.recv() {
                    return Ok(entries);
                }
            }
        }

        // 渲染UI
        terminal.draw(|frame| render_scan_ui(frame, &current_status))?;

        // 处理按键事件
        if let event::Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break Ok(Vec::new()),
                    _ => {}
                }
            }
        }
    })
}
// 渲染扫描UI
fn render_scan_ui(frame: &mut Frame, status: &ScanStatus) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3), // 标题
                Constraint::Length(3), // 进度条
                Constraint::Length(3), // 当前状态
                Constraint::Length(3), // 统计信息
                Constraint::Fill(1),   // 填充剩余空间
            ]
            .as_ref(),
        )
        .split(frame.area());
    log::info!("渲染UI:{:?}", status);
    // 标题
    let title = Paragraph::new("Directory Scanner")
        .block(Block::default().borders(Borders::ALL))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // 根据状态渲染不同内容
    match status {
        ScanStatus::Scanning {
            current_path,
            progress,
        } => {
            // // 进度条
            // let progress_bar = Paragraph::new(vec![
            //     Span::raw("Progress: "),
            //     Span::styled(format!("{}%", progress), Style::default().fg(Color::Green)),
            // ])
            // 修改后
            let progress_bar = Paragraph::new(Line::from(vec![
                Span::raw("Progress: "),
                Span::styled(format!("{}%", progress), Style::default().fg(Color::Green)),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Scan Progress"),
            )
            .alignment(Alignment::Center);
            frame.render_widget(progress_bar, chunks[1]);

            // 当前路径
            let path_text = Paragraph::new(current_path.as_str())
                .block(Block::default().borders(Borders::ALL).title("Current Path"))
                .wrap(Wrap { trim: true });
            frame.render_widget(path_text, chunks[2]);
        }
        ScanStatus::Completed {
            total_files,
            total_size,
        } => {
            // 完成之后
            let completion_text = Paragraph::new(Line::from(vec![
                Span::raw("Scan completed!"),
                Span::styled(
                    format!("\nTotal files: {}", total_files),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(
                    format!("\nTotal size: {}", total_size),
                    Style::default().fg(Color::Green),
                ),
            ]))
            .block(Block::default().borders(Borders::ALL).title("Scan Results"))
            .alignment(Alignment::Center);
            frame.render_widget(completion_text, chunks[1]);

            // 提示信息
            let hint_text = Paragraph::new("Press any key to continue...")
                .block(Block::default().borders(Borders::ALL))
                .alignment(Alignment::Center);
            frame.render_widget(hint_text, chunks[2]);
        }
    }
}

fn init_render(entries: Vec<FileEntry>) -> color_eyre::Result<()> {
    color_eyre::install()?;

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
                    KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                    _ => {}
                }
            }
        }
    })
}

/// Render the UI with various lists.
fn render(frame: &mut Frame, list_state: &mut ListState, entries: &[FileEntry]) {
    let constraints = [
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ];
    let layout = Layout::vertical(constraints).spacing(1);
    let [top, first, second] = frame.area().layout(&layout);

    let title = Line::from_iter([
        Span::from("List Widget").bold(),
        Span::from(" (Press 'q' to quit and arrow keys to navigate)"),
    ]);
    frame.render_widget(title.centered(), top);

    render_list(frame, first, list_state, entries);
}

/// Render a list.
pub fn render_list(
    frame: &mut Frame,
    area: Rect,
    list_state: &mut ListState,
    entries: &[FileEntry],
) {
    let items: Vec<Line> = entries
        .iter()
        .map(|entry| {
            Line::from(vec![
                Span::styled(
                    format!("{} ", entry.file_type),
                    Style::default().fg(if entry.file_type == 'd' {
                        Color::Blue
                    } else {
                        Color::White
                    }),
                ),
                Span::styled(
                    format!("{:<10} ", entry.size_display),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(entry.path.clone(), Style::default()),
            ])
        })
        .collect();

    let list = List::new(items)
        .style(Color::White)
        .highlight_style(Modifier::REVERSED)
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
