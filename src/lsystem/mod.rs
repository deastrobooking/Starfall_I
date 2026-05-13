pub mod tree;
/// L-system string rewriter + 3-D turtle interpreter.
///
/// Usage flow:
///   1. Build an `LSystem` with axiom, production rules, and geometric params.
///   2. Call `.generate()` to get the fully rewritten string.
///   3. Build a `Turtle` from the same params and call `.interpret(&string)`.
///   4. Pass the `TurtleResult` to `tree::spawn_tree()` to create Bevy entities.
pub mod turtle;

// ── String rewriter ───────────────────────────────────────────────────────────

/// One production rule: every occurrence of `symbol` is replaced by `expansion`.
#[derive(Debug, Clone)]
pub struct Rule {
    pub symbol: char,
    pub expansion: String,
}

/// An L-system definition.
#[derive(Debug, Clone)]
pub struct LSystem {
    pub axiom: String,
    pub rules: Vec<Rule>,
    pub iterations: u32,
    /// Turtle rotation angle in degrees.
    pub angle_deg: f32,
    /// Length of one `F` step in world units.
    pub step: f32,
    /// Factor applied to `step` when entering a branch (push `[`).
    pub length_scale: f32,
    /// Starting branch radius in world units.
    pub start_radius: f32,
    /// Factor applied to `radius` when entering a branch.
    pub radius_scale: f32,
}

impl LSystem {
    /// Construct from lightweight slice literals.
    pub fn new(
        axiom: &str,
        rules: &[(&str, &str)], // (symbol_str, expansion)
        iterations: u32,
        angle_deg: f32,
        step: f32,
        length_scale: f32,
        start_radius: f32,
        radius_scale: f32,
    ) -> Self {
        Self {
            axiom: axiom.to_string(),
            rules: rules
                .iter()
                .map(|(s, e)| Rule {
                    symbol: s.chars().next().expect("rule symbol must be non-empty"),
                    expansion: e.to_string(),
                })
                .collect(),
            iterations,
            angle_deg,
            step,
            length_scale,
            start_radius,
            radius_scale,
        }
    }

    /// Run the production rules for `self.iterations` steps and return the result string.
    pub fn generate(&self) -> String {
        let mut current = self.axiom.clone();
        for _ in 0..self.iterations {
            let mut next = String::with_capacity(current.len() * 4);
            for ch in current.chars() {
                match self.rules.iter().find(|r| r.symbol == ch) {
                    Some(r) => next.push_str(&r.expansion),
                    None => next.push(ch),
                }
            }
            current = next;
        }
        current
    }

    /// Build the matching `Turtle` from this system's geometric params.
    pub fn turtle(&self) -> turtle::Turtle {
        turtle::Turtle::new(
            self.angle_deg,
            self.step,
            self.length_scale,
            self.start_radius,
            self.radius_scale,
        )
    }

    /// Shortcut: generate string then interpret it.
    pub fn evaluate(&self) -> turtle::TurtleResult {
        let s = self.generate();
        self.turtle().interpret(&s)
    }
}
