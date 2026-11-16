//! The card a server tag opens: the server it's from, as Discord shows it,
//! with a way there when this account is in it.

use super::*;

pub(super) fn bind(window: &ChatterWindow, adapter: &Rc<RefCell<Adapter>>) {
    let card = window.global::<TagCard>();
    {
        let (state, weak) = (adapter.clone(), window.as_weak());
        card.on_show(move |guild, text, badge, x, y| {
            let Some(window) = weak.upgrade() else { return };
            let adapter = state.borrow();
            let card = window.global::<TagCard>();
            let id = chatter_core::discord::server_id(&guild);
            let server = (!guild.is_empty())
                .then(|| adapter.app.state.servers.iter().find(|server| server.id == id))
                .flatten();
            card.set_guild(id.as_str().into());
            card.set_text(text);
            card.set_badge(badge);
            card.set_name(server.map_or_else(|| "Server tag".into(), |server| server.name.as_str().into()));
            card.set_icon(server.and_then(|server| server.icon.as_deref()).unwrap_or_default().into());
            card.set_banner(server.and_then(|server| server.banner.as_deref()).unwrap_or_default().into());
            card.set_joined(server.is_some());
            card.set_x(x);
            card.set_y(y);
            card.set_open(true);
        });
    }
    let (state, weak) = (adapter.clone(), window.as_weak());
    card.on_go_to_server(move |id| {
        let Some(window) = weak.upgrade() else { return };
        window.global::<TagCard>().set_open(false);
        state
            .borrow_mut()
            .action(&window, Action::Navigate(Navigation::Server(id.into())));
    });
}
