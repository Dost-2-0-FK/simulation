use std::{collections::HashMap, sync::Arc};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

use crate::{
    domain::{BaseId, MilitaryBase, MilitaryUnit, UnitId},
    geometry::Point,
    handlers::units::UnitResponse,
    services::payment_service::PaymentService,
};

pub(crate) async fn get(
    resp: Sender<Vec<UnitResponse>>,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
) {
    let unit_responses = stream::iter(units.values())
        .then(async |unit| {
            let unit_guard = unit.read().await;
            let base_guard = bases
                .get(&unit_guard.base_id())
                .expect("units always have a base")
                .read()
                .await;
            let base_response = (&(*base_guard)).into();
            UnitResponse::new(&unit_guard, Some(base_response))
        })
        .collect()
        .await;
    let _ = resp.send(unit_responses);
}

pub(crate) fn create(base_id: BaseId, position: Point, payment_service: &PaymentService) -> MilitaryUnit {
    let payment = payment_service.pay_for_military_unit();
    MilitaryUnit::new(payment, base_id, position)
}
