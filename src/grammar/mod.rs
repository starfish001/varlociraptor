use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::convert::TryFrom;
use std::ops::{Deref, DerefMut};

use anyhow::Result;
use serde_yaml;
use vec_map::VecMap;

pub mod formula;
pub mod vaftree;

use crate::errors;
pub use crate::grammar::formula::{Formula, VAFRange, VAFSpectrum, VAFUniverse};
pub use crate::grammar::vaftree::VAFTree;

/// Container for arbitrary sample information.
/// Use `varlociraptor::grammar::Scenario::sample_info()` to create it.
#[derive(Clone, Debug)]
pub struct SampleInfo<T> {
    inner: Vec<T>,
}

impl<T> SampleInfo<T> {
    /// Map to other value type.
    pub fn map<U, F: Fn(&T) -> U>(&self, f: F) -> SampleInfo<U> {
        SampleInfo {
            inner: self.inner.iter().map(f).collect(),
        }
    }

    /// Map to other value type or fail at first error.
    pub fn try_map<U, E, F: Fn(&T) -> Result<U, E>>(&self, f: F) -> Result<SampleInfo<U>, E> {
        let mut inner = Vec::with_capacity(self.inner.len());
        for res in self.inner.iter().map(f) {
            inner.push(res?);
        }
        Ok(SampleInfo { inner: inner })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.inner.iter_mut()
    }
}

impl<T> Default for SampleInfo<T> {
    fn default() -> Self {
        SampleInfo {
            inner: Vec::default(),
        }
    }
}

impl<T> Deref for SampleInfo<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for SampleInfo<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Builder for `SampleInfo`.
#[derive(new, Debug)]
pub struct SampleInfoBuilder<T> {
    #[new(default)]
    inner: VecMap<T>,
    sample_idx: HashMap<String, usize>,
}

impl<T> SampleInfoBuilder<T> {
    pub fn push(mut self, sample_name: &str, value: T) -> Self {
        let idx = *self
            .sample_idx
            .get(sample_name)
            .expect("unknown sample name, it does not occur in the scenario");
        self.inner.insert(idx, value);

        self
    }

    pub fn build(self) -> SampleInfo<T> {
        SampleInfo {
            inner: self.inner.into_iter().map(|(_, v)| v).collect(),
        }
    }
}

#[derive(Deserialize, Getters)]
#[get = "pub"]
pub struct Scenario {
    // map of events
    events: BTreeMap<String, Formula>,
    // map of samples
    samples: BTreeMap<String, Sample>,
    #[serde(skip)]
    sample_idx: RefCell<Option<HashMap<String, usize>>>,
}

impl Scenario {
    pub fn sample_info<T>(&self) -> SampleInfoBuilder<T> {
        if self.sample_idx.borrow().is_none() {
            self.sample_idx.borrow_mut().get_or_insert(
                self.samples()
                    .keys()
                    .enumerate()
                    .map(|(i, s)| (s.to_owned(), i))
                    .collect(),
            );
        }
        SampleInfoBuilder::new(self.sample_idx.borrow().as_ref().unwrap().clone())
    }

    pub fn idx(&self, sample: &str) -> Option<usize> {
        if self.sample_idx.borrow().is_none() {
            self.sample_idx.borrow_mut().get_or_insert(
                self.samples()
                    .keys()
                    .enumerate()
                    .map(|(i, s)| (s.to_owned(), i))
                    .collect(),
            );
        }
        self.sample_idx
            .borrow()
            .as_ref()
            .unwrap()
            .get(sample)
            .map(|idx| *idx)
    }

    pub fn vaftrees(&self, contig: &str) -> Result<HashMap<String, VAFTree>> {
        self.events()
            .iter()
            .map(|(name, formula)| {
                let normalized = formula.normalize(self, contig)?;
                let vaftree = VAFTree::new(&normalized, self, contig)?;
                Ok((name.to_owned(), vaftree))
            })
            .collect()
    }
}

impl<'a> TryFrom<&'a str> for Scenario {
    type Error = serde_yaml::Error;

    fn try_from(yaml: &str) -> Result<Self, Self::Error> {
        serde_yaml::from_str(yaml)
    }
}

#[derive(Deserialize, Getters)]
#[get = "pub"]
pub struct Sample {
    /// optional group name
    group: Option<String>,
    /// optional contamination
    contamination: Option<Contamination>,
    /// grid point resolution for integration over continuous allele frequency ranges
    resolution: usize,
    /// possible VAFs of given sample
    universe: UniverseDefinition,
}

impl Sample {
    pub fn contig_universe(&self, contig: &str) -> Result<&VAFUniverse> {
        Ok(match self.universe {
            UniverseDefinition::Simple(ref universe) => universe,
            UniverseDefinition::Map(ref map) => match map.get(contig) {
                Some(universe) => universe,
                None => map
                    .get("all")
                    .ok_or_else(|| errors::Error::UniverseContigNotFound {
                        contig: contig.to_owned(),
                    })?,
            },
        })
    }
}

#[derive(Deserialize, Getters)]
#[get = "pub"]
pub struct Contamination {
    /// name of contaminating sample
    by: String,
    /// fraction of contamination
    fraction: f64,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum UniverseDefinition {
    Map(BTreeMap<String, VAFUniverse>),
    Simple(VAFUniverse),
}
