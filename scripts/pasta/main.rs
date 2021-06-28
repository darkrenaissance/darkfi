use pasta_curves as pasta;
use group::{Group, Curve};
use rand::rngs::OsRng;

fn main() {
    let g = pasta::vesta::Point::generator();
    println!("G = {:?}", g.to_affine());
    let x = pasta::vesta::Scalar::from(87u64);
    println!("x = 87 = {:?}", x);
    let b = g * x;
    println!("B = xG = {:?}", b.to_affine());

    let y = x - pasta::vesta::Scalar::from(90u64);
    println!("y = x - 90 = {:?}", y);

    let c = pasta::vesta::Point::random(&mut OsRng);
    let d = pasta::vesta::Point::random(&mut OsRng);
    println!("C = {:?}", c.to_affine());
}
