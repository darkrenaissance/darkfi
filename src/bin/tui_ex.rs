use drk::{
    tui::{App, HBox, VBox, Widget},
    Result,
};

async fn start() -> Result<()> {
    let wv1 = vec![Widget::new("V1".into())?];

    let wh1 = vec![Widget::new("H1".into())?];

    let wv2 = vec![Widget::new("V2".into())?];

    let wv3 = vec![Widget::new("V3".into())?, Widget::new("V4".into())?];

    let v_box1 = Box::new(VBox::new(wv1.clone(), 2));
    let h_box1 = Box::new(HBox::new(wh1.clone(), 2));
    let v_box2 = Box::new(VBox::new(wv2.clone(), 2));
    let v_box3 = Box::new(VBox::new(wv3.clone(), 1));

    let mut app = App::new()?;

    app.add_layout(v_box1)?;
    app.add_layout(h_box1)?;
    app.add_layout(v_box2)?;
    app.add_layout(v_box3)?;

    app.run().await?;

    Ok(())
}

fn main() -> Result<()> {
    smol::future::block_on(start())
}
