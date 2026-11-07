//! Settings, My server: connect this app to your own Linux server in one
//! field, see what it can run, and put the open project there.
//!
//! The connecting itself happens in [`super::remote`]; this is its page.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::remote::{self, Step};
use super::ui::{self, c, space};
use super::Shell;

fn line_row(title: impl Into<SharedString>, line: impl Into<SharedString>) -> Div {
    div()
        .px(px(18.))
        .py(px(14.))
        .flex()
        .flex_col()
        .gap(px(3.))
        .border_t_1()
        .border_color(c(ui::line2()))
        .child(
            div()
                .text_size(px(13.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c(ui::ink()))
                .child(title.into()),
        )
        .child(
            div()
                .text_size(px(12.5))
                .text_color(c(ui::muted()))
                .child(line.into()),
        )
}

fn card(title: &'static str) -> Div {
    ui::card().overflow_hidden().flex().flex_col().child(
        div()
            .px(px(18.))
            .pt(px(14.))
            .pb(px(10.))
            .text_size(px(11.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(c(ui::muted()))
            .child(title),
    )
}

impl Shell {
    /// Reads how connecting is going, and starts the link once it's paired.
    pub(super) fn poll_server_setup(&mut self) {
        let Some(rx) = &self.server_setup else {
            return;
        };
        let steps: Vec<Step> = rx.try_iter().collect();
        let mut done = false;
        for step in steps {
            match step {
                Step::Say(t) => self.server_log.push(t),
                Step::Done(cfg) => {
                    self.server_log.clear();
                    let label = cfg.label();
                    self.crew.link = Some(remote::Link::start(cfg));
                    self.say(format!(
                        "Connected to {label}. Jobs that suit it run there now."
                    ));
                    done = true;
                }
                Step::Failed(why) => {
                    self.server_log.push(why);
                    done = true;
                }
            }
        }
        if done {
            self.server_setup = None;
        }
    }

    fn connect_server(&mut self, cx: &mut Context<Self>) {
        if self.server_setup.is_some() {
            return;
        }
        let host = self.server_input.read(cx).value().trim().to_string();
        self.server_log.clear();
        if !remote::valid_host(&host) {
            self.server_log
                .push("Type it the way you'd type it after ssh, like you@203.0.113.7.".into());
            cx.notify();
            return;
        }
        self.server_setup = Some(remote::connect(host, remote::device_name()));
        cx.notify();
    }

    pub(super) fn settings_server(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let body = match &self.crew.link {
            None => self.server_connect_card(cx),
            Some(_) => self.server_connected(cx),
        };
        Self::page(
            "settings-server",
            "My server",
            "Run jobs on your own Linux server, so they carry on with this computer closed. Squad reaches it over SSH with your key, and nothing on it is opened to the internet.",
            body,
        )
    }

    fn server_connect_card(&mut self, cx: &mut Context<Self>) -> Div {
        let busy = self.server_setup.is_some();
        let failed = !busy && !self.server_log.is_empty();
        let field = div()
            .px(px(18.))
            .py(px(14.))
            .flex()
            .items_center()
            .gap(px(space::M))
            .border_t_1()
            .border_color(c(ui::line2()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(36.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(8.))
                    .bg(c(ui::bg()))
                    .border_1()
                    .border_color(c(ui::line()))
                    .text_size(px(13.))
                    .child(
                        Input::new(&self.server_input)
                            .appearance(false)
                            .bordered(false),
                    ),
            )
            .child(
                ui::solid_i(
                    "server-connect",
                    IconName::Server,
                    if busy { "Connecting…" } else { "Connect" },
                )
                .when(busy, |d| d.opacity(0.6))
                .on_click(cx.listener(|this, _, _, cx| this.connect_server(cx))),
            );
        let log = div()
            .px(px(18.))
            .pb(px(14.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .children(self.server_log.iter().enumerate().map(|(i, l)| {
                let last = i + 1 == self.server_log.len();
                div()
                    .text_size(px(12.5))
                    .text_color(c(if last && failed {
                        ui::t_red()
                    } else {
                        ui::muted()
                    }))
                    .child(l.clone())
            }));
        div()
            .flex()
            .flex_col()
            .gap(px(space::M))
            .child(
                card("YOUR SERVER")
                    .child(field)
                    .when(!self.server_log.is_empty(), |d| d.child(log)),
            )
            .child(
                card("WHAT CONNECTING DOES")
                    .child(line_row(
                        "Installs Squad Code there",
                        "Into ~/.local/bin, without root, and keeps it running as a service that comes back after a restart.",
                    ))
                    .child(line_row(
                        "Pairs this computer",
                        "With a one-time code fetched over SSH, so there's nothing to copy. The server only listens on its own address; this app reaches it through an SSH tunnel.",
                    ))
                    .child(line_row(
                        "Needs",
                        "A Linux server you can `ssh` into with a key (no password prompt), such as a small VPS.",
                    )),
            )
    }

    fn server_connected(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(link) = &self.crew.link else {
            return div();
        };
        let label = link.cfg.label();
        let host = link.cfg.host.clone();
        let up = link.up();
        let engines = link.engines();
        let projects = link.projects();
        let here = self.crew.store.folder().cloned();
        // The open project, when the server doesn't have it yet and it can be cloned there.
        let copy = here.and_then(|f| {
            let url = super::ship::remote_url(&f.path)?;
            link.path_for(&f, &url)
                .is_none()
                .then_some((f.name.clone(), url))
        });
        let status = div()
            .px(px(18.))
            .py(px(14.))
            .flex()
            .items_center()
            .gap(px(space::M))
            .border_t_1()
            .border_color(c(ui::line2()))
            .child(div().size(px(8.)).rounded_full().bg(c(if up {
                ui::t_green()
            } else {
                ui::faint()
            })))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c(ui::ink()))
                            .child(host.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(c(ui::muted()))
                            .child(link.line()),
                    ),
            )
            .child(
                ui::ghost("server-disconnect", "Disconnect").on_click(cx.listener(
                    |this, _, _, cx| {
                        remote::forget();
                        this.crew.link = None;
                        this.say("Disconnected. The server keeps running; connect again any time.");
                        cx.notify();
                    },
                )),
            );
        let agents = if engines.is_empty() {
            format!("None signed in yet. Run `ssh {host}`, then sign in to an agent there, such as `claude`.")
        } else {
            engines
                .iter()
                .map(|id| {
                    crate::agent::PROVIDERS
                        .iter()
                        .find(|p| p.id == id.as_str())
                        .map(|p| p.name.to_string())
                        .unwrap_or_else(|| id.clone())
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let names = if projects.is_empty() {
            "None yet. Jobs only run there on projects it has.".to_string()
        } else {
            projects
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut projects_row = div()
            .px(px(18.))
            .py(px(14.))
            .flex()
            .items_center()
            .gap(px(space::M))
            .border_t_1()
            .border_color(c(ui::line2()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c(ui::ink()))
                            .child("Projects there"),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(c(ui::muted()))
                            .child(names),
                    ),
            );
        if let Some((name, url)) = copy.filter(|_| up) {
            projects_row = projects_row.child(
                ui::ghost_i(
                    "server-copy",
                    IconName::CloudUpload,
                    format!("Put {name} there"),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(link) = &this.crew.link {
                        link.add_project(&name, &url);
                        this.say(format!("Cloning {name} on the server…"));
                    }
                    cx.notify();
                })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(space::M))
            .child(
                card("YOUR SERVER")
                    .child(status)
                    .child(line_row("Agents signed in there", agents))
                    .child(projects_row),
            )
            .child(card("HOW JOBS PICK A SIDE").child(line_row(
                format!("Here or on {label}"),
                "A job runs where its project is. When both sides have it, long work (big rewrites, migrations, overnight jobs) goes to the server and the rest stays here. Each job's feed says where it ran and why, and if the server can't take it, it runs here.",
            )))
    }
}
