use ff::PrimeField;
use group::Group;
use jubjub::SubgroupPoint;

fn main() {
    let g = SubgroupPoint::from_raw_unchecked(
        bls12_381::Scalar::from_raw([
            0xb981_9dc8_2d90_607e,
            0xa361_ee3f_d48f_df77,
            0x52a3_5a8c_1908_dd87,
            0x15a3_6d1f_0f39_0d88,
        ]),
        bls12_381::Scalar::from_raw([
            0x7b0d_c53c_4ebf_1891,
            0x1f3a_beeb_98fa_d3e8,
            0xf789_1142_c001_d925,
            0x015d_8c7f_5b43_fe33,
        ]),
    );
    let x = g + g;
    let x = jubjub::AffinePoint::from(jubjub::ExtendedPoint::from(x));
    println!("{:?}", x);
}

