
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::models::BeetleInventory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum BeetleCategory {
    Cheese = 1,
    Beetles = 2,
    Trash = 3,
    Hammers = 4,
    Flowers = 5,
    Unique = 7
}

impl BeetleCategory {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(BeetleCategory::Cheese),
            2 => Some(BeetleCategory::Beetles),
            3 => Some(BeetleCategory::Trash),
            4 => Some(BeetleCategory::Hammers),
            5 => Some(BeetleCategory::Flowers),
            7 => Some(BeetleCategory::Unique),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            BeetleCategory::Cheese => "Cheese",
            BeetleCategory::Beetles => "Beetles",
            BeetleCategory::Trash => "Trash",
            BeetleCategory::Hammers => "Hammers",
            BeetleCategory::Flowers => "Flowers",
            BeetleCategory::Unique => "Unique",
        }
    }
}

pub type BeetleDatabase = HashMap<String, BeetleData>;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BeetleData {
    pub beetle: String,
    pub category: u8,
    pub rarity: u8,
}

impl BeetleData {
    pub fn get_category(&self) -> Option<BeetleCategory> {
        BeetleCategory::from_u8(self.category)
    }
}

#[derive(Debug, Clone)]
pub struct InventoryItem {
    pub key: String,
    pub quantity: i64,
    pub data: BeetleData,
}

#[derive(Debug, Clone)]
pub struct OrganizedInventory {
    pub items: Vec<InventoryItem>,
    pub by_category: HashMap<BeetleCategory, Vec<InventoryItem>>,
    pub total_items: i64,
}

impl OrganizedInventory {

    pub fn from_inventory(
        inventory: &BeetleInventory,
        database: &BeetleDatabase,
     ) -> Self {
        let mut items = Vec::new();
        let mut by_category: HashMap<BeetleCategory, Vec<InventoryItem>> = HashMap::new();
        let mut total_items = 0i64;

        // Create a list of (key, quantity) tuples from the inventory struct
        let inventory_items = vec![
            ("beetleboy_key", inventory.beetleboy_key),
            ("bike_reflector", inventory.bike_reflector),
            ("bombardier", inventory.bombardier),
            ("bottle_cap", inventory.bottle_cap),
            ("camellia", inventory.camellia),
            ("cheese", inventory.cheese),
            ("chip_bag", inventory.chip_bag),
            ("chocolate_bar", inventory.chocolate_bar),
            ("chocolate_wrapper", inventory.chocolate_wrapper),
            ("christmas", inventory.christmas),
            ("cigarette_butt", inventory.cigarette_butt),
            ("coffee_can", inventory.coffee_can),
            ("cracker_wrapper", inventory.cracker_wrapper),
            ("daisy", inventory.daisy),
            ("empty_noodle_cup", inventory.empty_noodle_cup),
            ("gallic_rose", inventory.gallic_rose),
            ("giraffe_weevil", inventory.giraffe_weevil),
            ("green", inventory.green),
            ("green_army_man", inventory.green_army_man),
            ("gum_wrapper", inventory.gum_wrapper),
            ("hammer_t1", inventory.hammer_t1),
            ("hammer_t2", inventory.hammer_t2),
            ("hammer_t3", inventory.hammer_t3),
            ("hammer_t4", inventory.hammer_t4),
            ("imperial_tortoise", inventory.imperial_tortoise),
            ("jack_adapter", inventory.jack_adapter),
            ("juicebox", inventory.juicebox),
            // ("junk_cube_t1", inventory.junk_cube_t1),
            // ("junk_cube_t2", inventory.junk_cube_t2),
            ("ladybug", inventory.ladybug),
            ("marble", inventory.marble),
            ("marigold", inventory.marigold),
            ("milk_thistle", inventory.milk_thistle),
            ("monarch", inventory.monarch),
            ("morning_glory", inventory.morning_glory),
            ("paperclip", inventory.paperclip),
            ("pebble", inventory.pebble),
            ("pillbug", inventory.pillbug),
            ("pincushion", inventory.pincushion),
            ("pokkiri_box", inventory.pokkiri_box),
            ("pollen_common", inventory.pollen_common),
            ("pollen_rare", inventory.pollen_rare),
            ("pollen_uncommon", inventory.pollen_uncommon),
            ("pond", inventory.pond),
            ("poppy", inventory.poppy),
            ("purple", inventory.purple),
            ("ramune_bottle", inventory.ramune_bottle),
            ("red_whistle", inventory.red_whistle),
            ("royal_poinciana", inventory.royal_poinciana),
            ("rubber_band", inventory.rubber_band),
            ("sabertooth_longhorn", inventory.sabertooth_longhorn),
            ("scratch_off", inventory.scratch_off),
            ("skull", inventory.skull),
            ("smiley_pebble", inventory.smiley_pebble),
            ("soda_can_tab", inventory.soda_can_tab),
            ("stag", inventory.stag),
            ("stamp", inventory.stamp),
            ("sunflower", inventory.sunflower),
            ("train_ticket_stub", inventory.train_ticket_stub),
            ("watch_battery", inventory.watch_battery),
            ("wine_cork", inventory.wine_cork),
            ("golden", inventory.golden),
            ("goliath", inventory.goliath),
            ("black_widow", inventory.black_widow),
        ];

        for (key, quantity) in inventory_items {
            // Skip items with 0 quantity
            if quantity <= 0 {
                continue;
            }

            // Look up beetle data in database
            if let Some(beetle_data) = database.get(key) {
                let item = InventoryItem {
                    key: key.to_string(),
                    quantity,
                    data: beetle_data.clone(),
                };

                total_items += quantity;
                items.push(item.clone());

                // Organize by category
                if let Some(category) = beetle_data.get_category() {
                    by_category
                        .entry(category)
                        .or_insert_with(Vec::new)
                        .push(item);
                }
            } else {
                eprintln!("Warning: Unknown beetle key in inventory: {}", key);
            }
        }

        Self {
            items,
            by_category,
            total_items,
        }
    }

        /// Get all items in a specific category
    pub fn get_category(&self, category: BeetleCategory) -> Option<&Vec<InventoryItem>> {
        self.by_category.get(&category)
    }

    /// Get all trash items
    pub fn get_trash(&self) -> Option<&Vec<InventoryItem>> {
        self.get_category(BeetleCategory::Trash)
    }

    /// Get all flowers
    pub fn get_flowers(&self) -> Option<&Vec<InventoryItem>> {
        self.get_category(BeetleCategory::Flowers)
    }

    /// Get all beetles
    pub fn get_beetles(&self) -> Option<&Vec<InventoryItem>> {
        self.get_category(BeetleCategory::Beetles)
    }

    /// Get all hammers
    pub fn get_hammers(&self) -> Option<&Vec<InventoryItem>> {
        self.get_category(BeetleCategory::Hammers)
    }

    /// Get category sorted by rarity (highest first)
    pub fn get_category_sorted_by_rarity(
        &self,
        category: BeetleCategory,
    ) -> Option<Vec<InventoryItem>> {
        self.get_category(category).map(|items| {
            let mut sorted = items.clone();
            sorted.sort_by(|a, b| b.data.rarity.cmp(&a.data.rarity));
            sorted
        })
    }

    /// Get category sorted by quantity (highest first)
    pub fn get_category_sorted_by_quantity(
        &self,
        category: BeetleCategory,
    ) -> Option<Vec<InventoryItem>> {
        self.get_category(category).map(|items| {
            let mut sorted = items.clone();
            sorted.sort_by(|a, b| b.quantity.cmp(&a.quantity));
            sorted
        })
    }

    /// Get total quantity for a category
    pub fn get_category_total(&self, category: BeetleCategory) -> i64 {
        self.get_category(category)
            .map(|items| items.iter().map(|item| item.quantity).sum())
            .unwrap_or(0)
    }

    /// Get statistics per category
    // pub fn get_category_stats(&self) -> HashMap<BeetleCategory, CategoryStats> {
    //     let mut stats = HashMap::new();

    //     for (category, items) in &self.by_category {
    //         let total_quantity: i64 = items.iter().map(|item| item.quantity).sum();
    //         let unique_items = items.len();
    //         let avg_rarity = if !items.is_empty() {
    //             items.iter().map(|item| item.data.rarity as f64).sum::<f64>()
    //                 / items.len() as f64
    //         } else {
    //             0.0
    //         };

    //         stats.insert(
    //             *category,
    //             CategoryStats {
    //                 category: *category,
    //                 total_quantity,
    //                 unique_items,
    //                 avg_rarity,
    //             },
    //         );
    //     }

    //     stats
    // }

    /// Get all items sorted by rarity
    pub fn get_all_sorted_by_rarity(&self) -> Vec<InventoryItem> {
        let mut sorted = self.items.clone();
        sorted.sort_by(|a, b| {
            b.data.rarity.cmp(&a.data.rarity)
                .then_with(|| b.quantity.cmp(&a.quantity))
        });
        sorted
    }

    /// Find specific item by key
    pub fn find_item(&self, key: &str) -> Option<&InventoryItem> {
        self.items.iter().find(|item| item.key == key)
    }
    
}


pub mod utils {
    use std::fs;

    use super::*;

    pub fn load_beetle_database(path: &str) -> Result<BeetleDatabase, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let database: BeetleDatabase = serde_json::from_str(&content)?;
        Ok(database)
    }
}