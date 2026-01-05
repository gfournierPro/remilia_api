use serde::{Deserialize, Serialize};

use crate::models::{BeetleCard, User};


#[derive(Debug, Serialize, Deserialize)]
pub struct CraftRequest {
    #[serde(rename = "type")]
    pub typeName: u32,
    pub slot1: String,
    pub slot2: String,
    pub slot3: String,
    pub slot4: String,
    pub sacrifice: String, 
    pub hammer: String, 
}


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftResponse {
    pub success: bool,
    pub message: String,
    #[serde(rename = "hammer_broke")]
    pub hammer_broke: bool,
    pub user: User,
    pub result: BeetleCard,
}



