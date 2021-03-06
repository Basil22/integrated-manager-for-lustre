// Copyright (c) 2019 DDN. All rights reserved.
// Use of this source code is governed by a MIT-style
// license that can be found in the LICENSE file.

use futures::{Future, FutureExt};
use iml_wire_types::Command;
use indicatif::ProgressBar;
use prettytable::{Row, Table};
use spinners::{Spinner, Spinners};
use std::fmt::Display;

pub fn wrap_fut<T>(msg: &str, fut: impl Future<Output = T>) -> impl Future<Output = T> {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(100);
    pb.set_message(msg);

    fut.inspect(move |_| pb.finish_and_clear())
}

pub fn start_spinner(msg: &str) -> impl FnOnce(Option<String>) -> () {
    let grey = termion::color::Fg(termion::color::LightBlack);
    let reset = termion::color::Fg(termion::color::Reset);

    let s = format!("{}{}{}", grey, reset, msg);
    let s_len = s.len();

    let sp = Spinner::new(Spinners::Dots9, s);

    move |msg_opt| match msg_opt {
        Some(msg) => {
            sp.message(msg);
        }
        None => {
            sp.stop();
            print!("{}", termion::clear::CurrentLine);
            print!("{}", termion::cursor::Left(s_len as u16));
        }
    }
}

pub fn format_cmd_state(cmd: &Command) -> String {
    if cmd.errored {
        format_error(format!("{} errored", cmd.message))
    } else if cmd.cancelled {
        format_cancelled(&format!("{} cancelled", cmd.message))
    } else {
        format_success(format!("{} successful", cmd.message))
    }
}

pub fn display_cmd_state(cmd: &Command) {
    println!("{}", format_cmd_state(&cmd));
}

pub fn format_cancelled(message: impl Display) -> String {
    format!("🚫 {}", message)
}

pub fn display_cancelled(message: impl Display) {
    println!("{}", format_cancelled(&message));
}

pub fn format_success(message: impl Display) -> String {
    let green = termion::color::Fg(termion::color::Green);
    let reset = termion::color::Fg(termion::color::Reset);

    format!("{}✔{} {}", green, reset, message)
}

pub fn display_success(message: impl Display) {
    println!("{}", format_success(message))
}

pub fn format_error(message: impl Display) -> String {
    let red = termion::color::Fg(termion::color::Red);
    let reset = termion::color::Fg(termion::color::Reset);

    format!("{}✗{} {}", red, reset, message)
}

pub fn display_error(message: impl Display) {
    println!("{}", format_error(message))
}

pub fn generate_table<Rows, R>(columns: &[&str], rows: Rows) -> Table
where
    R: IntoIterator,
    R::Item: ToString,
    Rows: IntoIterator<Item = R>,
{
    let mut table = Table::new();

    table.add_row(Row::from(columns));

    for r in rows {
        table.add_row(Row::from(r));
    }

    table
}
