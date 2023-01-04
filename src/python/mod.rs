//! Wrappers for using hidefix in Python.

use crate::filters::byteorder::ToNative;
use byte_slice_cast::ToMutByteSlice;
use numpy::{PyArray, PyArray1};
use pyo3::{
    prelude::*,
    types::{PySlice, PyTuple},
};
use std::path::PathBuf;
use std::sync::Arc;

use crate::idx;
use crate::prelude::*;

#[pymodule]
fn hidefix(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Index>()?;
    Ok(())
}

#[pyclass]
#[derive(Debug)]
struct Index {
    idx: Arc<idx::Index<'static>>,
}

#[pymethods]
impl Index {
    #[new]
    pub fn new(p: PathBuf) -> PyResult<Index> {
        Ok(Index {
            idx: Arc::new(idx::Index::index(&p)?),
        })
    }

    pub fn dataset(&self, s: &str) -> Option<Dataset> {
        self.idx.dataset(s).map(|_| Dataset {
            idx: self.idx.clone(),
            ds: String::from(s),
        })
    }

    pub fn datasets(&self) -> Vec<String> {
        self.idx.datasets().keys().cloned().collect::<Vec<_>>()
    }

    fn __repr__(&self) -> String {
        format!(
            "Index(file: {:?}, datasets: {}",
            self.idx.path(),
            self.idx.datasets().len()
        )
    }
}

#[pyclass]
#[derive(Debug)]
struct Dataset {
    idx: Arc<idx::Index<'static>>,
    ds: String,
}

impl Dataset {
    fn read_py_array<'py, T>(
        &self,
        py: Python<'py>,
        ds: &idx::DatasetD<'_>,
        indices: &[u64],
        counts: &[u64],
    ) -> PyResult<&'py PyAny>
    where
        T: numpy::Element + ToMutByteSlice + 'py,
        [T]: ToNative,
    {
        let r = ds.as_par_reader(self.idx.path().unwrap())?;
        let (a, dst) = unsafe {
            let a = PyArray::<T, _>::new(
                py,
                counts
                    .iter()
                    .cloned()
                    .map(|d| d as usize)
                    .collect::<Vec<_>>(),
                false,
            );
            let dst = a.as_slice_mut()?;

            (a, dst)
        };

        r.values_to_par(Some(indices), Some(counts), dst)?;
        Ok(a.as_ref())
    }
}

#[pymethods]
impl Dataset {
    fn __repr__(&self) -> String {
        format!("Dataset (\"{}\")", self.ds)
    }

    fn __len__(&self) -> usize {
        self.idx.dataset(&self.ds).unwrap().size()
    }

    fn shape<'py>(&self, py: Python<'py>) -> &'py PyArray1<u64> {
        PyArray::from_slice(py, self.idx.dataset(&self.ds).unwrap().shape())
    }

    fn chunk_shape<'py>(&self, py: Python<'py>) -> &'py PyArray1<u64> {
        PyArray::from_slice(py, self.idx.dataset(&self.ds).unwrap().chunk_shape())
    }

    fn __getitem__<'py>(&self, py: Python<'py>, slice: &PyTuple) -> PyResult<&'py PyAny> {
        let ds = self.idx.dataset(&self.ds).unwrap();
        let shape = ds.shape();

        println!("dtype: {:?}", ds.dtype());
        println!("shape: {:?}", shape);

        // if there are fewer slices than dimensions they will be extended by the full dimension
        // when read.
        let (mut indices, (mut counts, mut strides)): (Vec<_>, (Vec<_>, Vec<_>)) = slice
            .iter()
            .map(|el| {
                el.downcast::<PySlice>()
                    .expect("__getitem__ only accepts slices")
            })
            .zip(shape)
            .map(|(slice, dim_sz)| {
                let i = slice
                    .indices(*dim_sz as i64)
                    .expect("slice could not be retrieced, too big for dimension?");
                (i.start as u64, ((i.stop - i.start) as u64, i.step as u64))
            })
            .unzip();

        indices.resize_with(shape.len(), || 0);
        strides.resize_with(shape.len(), || 1);
        counts.extend_from_slice(&shape[counts.len()..]);

        dbg!(&indices);
        dbg!(&counts);
        dbg!(&strides);

        // read the data into correct datatype, convert to pyarray and cast as pyany.
        match ds.dtype() {
            Datatype::UInt(sz) if sz == 1 => {
                self.read_py_array::<u8>(py, ds, &indices, &counts)
            }
            Datatype::UInt(sz) if sz == 2 => {
                self.read_py_array::<u16>(py, ds, &indices, &counts)
            }
            Datatype::UInt(sz) if sz == 4 => {
                self.read_py_array::<u32>(py, ds, &indices, &counts)
            }
            Datatype::UInt(sz) if sz == 8 => {
                self.read_py_array::<u64>(py, ds, &indices, &counts)
            }
            Datatype::Int(sz) if sz == 1 => {
                self.read_py_array::<i8>(py, ds, &indices, &counts)
            }
            Datatype::Int(sz) if sz == 2 => {
                self.read_py_array::<i16>(py, ds, &indices, &counts)
            }
            Datatype::Int(sz) if sz == 4 => {
                self.read_py_array::<i32>(py, ds, &indices, &counts)
            }
            Datatype::Int(sz) if sz == 8 => {
                self.read_py_array::<i64>(py, ds, &indices, &counts)
            }
            Datatype::Float(sz) if sz == 4 => {
                self.read_py_array::<f32>(py, ds, &indices, &counts)
            }
            Datatype::Float(sz) if sz == 8 => {
                self.read_py_array::<f64>(py, ds, &indices, &counts)
            }
            _ => unimplemented!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_coads() {
        Python::with_gil(|py| {
            let i = Index::new("tests/data/coads_climatology.nc4".into()).unwrap();
            let ds = i.dataset("SST").unwrap();

            let arr = ds.__getitem__(py, PyTuple::new(py, vec![PySlice::new(py, 0, 10, 1)]));
            println!("{:?}", arr);
        });
    }
}
