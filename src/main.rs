pub mod dir_listing;
pub mod logger;
pub mod models;
pub mod utils;
pub use dir_listing::{list_directory, search_and_display_interactive};

use clap::Parser;
use logger::init_logger;
use std::path::Path;

use crossterm::event::{self, KeyCode};
use models::Cli;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListDirection, ListState};
use ratatui::Frame;

fn main() -> Result<(), anyhow::Error> {
    init_logger();
    let _ = init_render();

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
        list_directory(path, &args);
    } else {
        println!("{}", path.display());
    }

    // if args.show_time {
    //     let duration = start_time.elapsed();
    //     log::warn!(
    //         "\n运行时间: {:.6}秒 ({}毫秒, {}纳秒)",
    //         duration.as_secs_f64(),
    //         duration.as_millis(),
    //         duration.as_nanos()
    //     );
    // }
    Ok(())
}

fn init_render() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let mut list_state = ListState::default().with_selected(Some(0));
    ratatui::run(|terminal| loop {
        terminal.draw(|frame| render(frame, &mut list_state))?;
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
fn render(frame: &mut Frame, list_state: &mut ListState) {
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

    render_list(frame, first, list_state);
}

/// Render a list.
pub fn render_list(frame: &mut Frame, area: Rect, list_state: &mut ListState) {
    let items = [
        "Item 1", "Item 2", "Item 3", "Item 4", "Item 5", "Item 6", "Item 7", "Item 8", "Item 9",
    ];
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
