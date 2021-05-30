use pasta_curves as pasta;
use group::{Group, Curve};

fn main() {
    let a = pasta::vesta::Point::generator();
    println!("a = {:?}", a.to_affine());
    let x = pasta::vesta::Scalar::from(87u64);
    println!("x = {:?}", x);
    let b = a * x;
    println!("b = {:?}", b.to_affine());

    let y = x - pasta::vesta::Scalar::from(90u64);
    println!("y = {:?}", y);
}
