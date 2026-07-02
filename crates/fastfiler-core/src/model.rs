//! アプリ状態の骨格。Phase 1 で本実装する (計画書 §5.2)。

use slotmap::new_key_type;

new_key_type! {
    /// ペインの識別子。GPUI 版の `EntityId` に相当する。
    /// 所有は常に `AppModel` の SlotMap にあり、ID はただの参照。
    pub struct PaneId;
}
