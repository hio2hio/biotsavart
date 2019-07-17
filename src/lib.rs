extern crate ndarray_linalg;
extern crate ndarray_parallel;
extern crate ndarray_stats;

#[macro_use]
extern crate ndarray;
extern crate indicatif;

use pyo3::prelude::*;
use pyo3::wrap_pyfunction;

use ndarray::prelude::*;
use ndarray::Zip;

use ndarray_parallel::prelude::*;

use ndarray_linalg::norm::Norm;

use std::f64;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::time::Instant;

fn convert(phi: Vec<Vec<Vec<f64>>>) -> Array3<f64> {
    let flattened: Vec<f64> = phi.concat().concat();
    let init = Array3::from_shape_vec((phi.len(), phi[0].len(), phi[0][0].len()), flattened);
    init.unwrap()
}

// [x1, y1, z1] x [x2, y2, z2] = (-y2 z1 + y1 z2, x2 z1 - x1 z2, -x2 y1 + x1 y2)
#[pyfunction]
/// Calculates the magnetic field, B, generated by a current density, J
///
/// Parameters
/// ----------
/// jx : ndarray
///     Values of Jx on a 3D grid. Has to be a matrix of size MxNxK.
/// jy : ndarray
///     Values of Jy on a 3D grid. Has to be a matrix of size MxNxK.
/// jz : ndarray
///     Values of Jz on a 3D grid. Has to be a matrix of size MxNxK.
/// x_cor : array_like 
///     X coordinates for the first dimension of the J values grid.
/// y_cor : array_like 
///     Y coordinates for the second dimension of the J values grid.
/// z_cor : array_like 
///     Z coordinates for the third dimension of the J values grid.
///
/// Returns
/// -------
/// B : tuple of array_like
///     tuple of Bx, By and Bz. Each list has to be reshaped to match the original size of J.
///
/// Note
/// ----
/// Parallelized through the use of ndarray-parallel.
fn biot(
    jx: Vec<Vec<Vec<f64>>>,
    jy: Vec<Vec<Vec<f64>>>,
    jz: Vec<Vec<Vec<f64>>>,
    x_cor: Vec<f64>,
    y_cor: Vec<f64>,
    z_cor: Vec<f64>,
) -> PyResult<(Vec<f64>, Vec<f64>, Vec<f64>)> {
    let jx = convert(jx);
    let jy = convert(jy);
    let jz = convert(jz);

    let mut b_x = Array3::<f64>::zeros(jx.dim());
    let mut b_y = Array3::<f64>::zeros(jy.dim());
    let mut b_z = Array3::<f64>::zeros(jz.dim());

    println!("starting calculations");
    Zip::indexed(&mut b_x)
        .and(&mut b_y)
        .and(&mut b_z)
        .par_apply(|idx, result_x, result_y, result_z| {
            let b_r = array![
                x_cor[idx.0] as f64,
                y_cor[idx.1] as f64,
                z_cor[idx.2] as f64
            ];

            for (xi, x) in x_cor.iter().enumerate() {
                for (yi, y) in y_cor.iter().enumerate() {
                    for (zi, z) in z_cor.iter().enumerate() {
                        let jx_val = &jx[[xi, yi, zi]];
                        let jy_val = &jy[[xi, yi, zi]];
                        let jz_val = &jz[[xi, yi, zi]];

                        let r_mark = array![*x, *y, *z];
                        let r = &b_r - &r_mark;
                        let r3 = r.norm_l2().powf(3.0);

                        if r3 != 0.0 {
                            *result_x += (-r[1] * jz_val + jy_val * r[2]) / &r3;
                            *result_y += (r[0] * jz_val - jx_val * r[2]) / &r3;
                            *result_z += (-r[0] * jy_val + jx_val * r[1]) / &r3;
                        }
                    }
                }
            }
        });

    println!("calculations done");
    println!("sums: ");
    println!("x: {}", b_x.sum());
    println!("y: {}", b_y.sum());
    println!("z: {}", b_z.sum());
    println!("=======");

    println!("shapes");
    println!("x: {:?}", b_x.shape());
    println!("y: {:?}", b_y.shape());
    println!("z: {:?}", b_z.shape());

    println!("writing to disk");
    export_jmol(&b_x, &b_y, &b_z, x_cor, y_cor, z_cor);

    println!("Done!");

    Ok((b_x.into_raw_vec(), b_y.into_raw_vec(), b_z.into_raw_vec()))
}

fn export_jmol(
    bx: &Array3<f64>,
    by: &Array3<f64>,
    bz: &Array3<f64>,
    x_cor: Vec<f64>,
    y_cor: Vec<f64>,
    z_cor: Vec<f64>,
) {
    let path = Path::new("./parallel.spt");
    let mut file = File::create(&path).expect("Unable to write to file!");
    let mut arrow_idx = 0;
    let step = 3;

    write!(file, "load \"file:$SCRIPT_PATH$/central_region.xyz\" \n").unwrap();
    write!(file, "write \"$SCRIPT_PATH$/central_region2.xyz\" \n").unwrap();
    write!(file, "load \"file:$SCRIPT_PATH$/central_region2.xyz\" \n").unwrap();

    let mut lengths = Vec::with_capacity(x_cor.len() * y_cor.len() * z_cor.len() / step);
    for (ix, _x) in x_cor.iter().enumerate().step_by(step) {
        for (iy, _y) in y_cor.iter().enumerate().step_by(step) {
            for (iz, _z) in z_cor.iter().enumerate().step_by(step) {
                let norm = (bx[[ix, iy, iz]].powf(2.0)
                    + by[[ix, iy, iz]].powf(2.0)
                    + bz[[ix, iy, iz]].powf(2.0))
                .sqrt();
                lengths.push(norm);
            }
        }
    }

    let max = lengths.iter().cloned().fold(f64::NAN, f64::max);
    let min = lengths.iter().cloned().fold(f64::NAN, f64::min);

    if &max == &min {
        println!("max and min the same. max: {}, min: {}", max, min);
    } else {
        lengths = lengths
            .iter()
            .map(|x| (x - min) as f64 / (max - min) as f64)
            .collect();
    }

    for (ix, x) in x_cor.iter().enumerate().step_by(step) {
        for (iy, y) in y_cor.iter().enumerate().step_by(step) {
            for (iz, z) in z_cor.iter().enumerate().step_by(step) {
                write!(
                    file,
                    "draw arrow{} arrow color [1,0,0] diameter {} {{ {},{},{} }} {{ {},{},{} }}\n",
                    arrow_idx,
                    lengths[arrow_idx],
                    (x - bx[[ix, iy, iz]]),
                    (y - by[[ix, iy, iz]]),
                    (z - bz[[ix, iy, iz]]),
                    (x + bx[[ix, iy, iz]]),
                    (y + by[[ix, iy, iz]]),
                    (z + bz[[ix, iy, iz]])
                )
                .expect("unable to write line");
                arrow_idx += 1;
            }
        }
    }

    write!(file, "set defaultdrawarrowscale 0.1 \n").unwrap();
    write!(file, "rotate 90 \n").unwrap();
    write!(file, "background white \n").unwrap();
}

#[pymodule]
fn libbiot_savart(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_wrapped(wrap_pyfunction!(biot))?;

    Ok(())
}
