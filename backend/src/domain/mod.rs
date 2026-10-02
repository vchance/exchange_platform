//! Pure domain rules. Nothing in this module touches the database or the
//! network: each state machine is a function from (current state, action,
//! actor) to a new state or a typed refusal (DESIGN.md §13.3).

pub mod contribution;
