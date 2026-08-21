use std::collections::BTreeMap;

use crate::{
    CalculationError, CalculationProvenance, CalculationRequest, CalculationResult,
    CelestialObject, EphemerisSource, HouseCusps, Position,
};

/// Provider boundary for complete chart calculations.
pub trait EphemerisAdapter {
    fn calculate(
        &self,
        request: &CalculationRequest,
    ) -> Result<CalculationResult, CalculationError>;
}

/// An in-memory adapter for deterministic contract and consumer tests.
#[derive(Clone, Debug)]
pub struct DeterministicMock {
    positions: BTreeMap<CelestialObject, Position>,
    houses: HouseCusps,
}

impl DeterministicMock {
    pub fn new(positions: BTreeMap<CelestialObject, Position>, houses: HouseCusps) -> Self {
        Self { positions, houses }
    }
}

impl EphemerisAdapter for DeterministicMock {
    fn calculate(
        &self,
        request: &CalculationRequest,
    ) -> Result<CalculationResult, CalculationError> {
        let mut positions = BTreeMap::new();
        for object in request.objects() {
            let position = self
                .positions
                .get(object)
                .copied()
                .ok_or(CalculationError::MissingObject(*object))?;
            positions.insert(*object, position);
        }
        let provenance = CalculationProvenance::new(
            "deterministic_mock",
            env!("CARGO_PKG_VERSION"),
            EphemerisSource::Synthetic,
            None,
        )?;
        CalculationResult::new(request, positions, self.houses.clone(), provenance)
    }
}
