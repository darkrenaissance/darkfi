// tui that lists:
//      all active nodes
//      their connections
//      recent messages
//
// uses rpc to get that info from nodes
//
// later: can open nodes in tabs

use drk::{
    tui::{App, HBox, VBox, Widget},
    Result,
};

async fn start() -> Result<()> {
    let wv1 = vec![Widget::new("Active nodes".into())?];

    let v_box1 = Box::new(VBox::new(wv1.clone(), 1));

    let mut app = App::new()?;

    app.add_layout(v_box1)?;

    app.run().await?;

    Ok(())
}

fn main() -> Result<()> {
    smol::future::block_on(start())
}
