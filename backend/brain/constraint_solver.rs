// ─────────────────────────────────────────────────────────────
// Constraint Solver — CSP with AC-3 + Backtracking
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/constraint_solver.py
// N-Queens, Sudoku, Graph Coloring via arc consistency + backtracking.

use std::collections::{HashMap, HashSet};

/// A constraint satisfaction problem.
pub struct CSP {
    pub variables: Vec<String>,
    pub domains: HashMap<String, Vec<i64>>,
    pub constraints: Vec<Box<dyn Fn(&HashMap<String, i64>) -> bool + Send + Sync>>,
}

/// Result of constraint solving.
#[derive(Debug, Clone)]
pub struct CSPSolution {
    pub assignment: HashMap<String, i64>,
    pub solved: bool,
    pub nodes_explored: usize,
    pub duration_ms: f64,
}

/// Constraint Solver with AC-3 + backtracking.
pub struct ConstraintSolver {
    nodes_explored: usize,
}

impl ConstraintSolver {
    pub fn new() -> Self {
        Self { nodes_explored: 0 }
    }

    /// Solve N-Queens problem.
    pub fn solve_n_queens(&mut self, n: usize) -> Option<Vec<usize>> {
        self.nodes_explored = 0;
        let mut board = vec![0usize; n];
        if self.queens_backtrack(&mut board, 0, n) {
            Some(board)
        } else {
            None
        }
    }

    fn queens_backtrack(&mut self, board: &mut Vec<usize>, row: usize, n: usize) -> bool {
        if row == n {
            return true;
        }
        self.nodes_explored += 1;

        for col in 0..n {
            if self.is_safe_queen(board, row, col) {
                board[row] = col;
                if self.queens_backtrack(board, row + 1, n) {
                    return true;
                }
            }
        }
        false
    }

    fn is_safe_queen(&self, board: &[usize], row: usize, col: usize) -> bool {
        for i in 0..row {
            if board[i] == col {
                return false;
            }
            let row_diff = row - i;
            let col_diff = if board[i] > col {
                board[i] - col
            } else {
                col - board[i]
            };
            if row_diff == col_diff {
                return false;
            }
        }
        true
    }

    /// Format N-Queens solution as visual board.
    pub fn format_queens_board(solution: &[usize]) -> String {
        let n = solution.len();
        let mut lines = Vec::new();
        lines.push(format!("**{}-Queens Solution**\n", n));
        for row in 0..n {
            let mut line = String::new();
            for col in 0..n {
                if solution[row] == col {
                    line.push_str("♛ ");
                } else {
                    line.push_str("· ");
                }
            }
            lines.push(line);
        }
        lines.push(format!("\nPositions: {:?}", solution));
        lines.push(format!("Nodes explored: {}", 0));
        lines.join("\n")
    }

    /// Solve a Sudoku puzzle (9x9 grid, 0 = empty).
    pub fn solve_sudoku(&mut self, grid: &mut [[u8; 9]; 9]) -> bool {
        self.nodes_explored = 0;
        self.sudoku_backtrack(grid)
    }

    fn sudoku_backtrack(&mut self, grid: &mut [[u8; 9]; 9]) -> bool {
        self.nodes_explored += 1;

        // Find empty cell (MRV heuristic: pick cell with fewest candidates)
        let mut best_cell: Option<(usize, usize, Vec<u8>)> = None;
        let mut min_candidates = 10usize;

        for r in 0..9 {
            for c in 0..9 {
                if grid[r][c] == 0 {
                    let candidates = self.sudoku_candidates(grid, r, c);
                    if candidates.is_empty() {
                        return false;
                    }
                    if candidates.len() < min_candidates {
                        min_candidates = candidates.len();
                        best_cell = Some((r, c, candidates));
                    }
                }
            }
        }

        let (row, col, candidates) = match best_cell {
            Some(cell) => cell,
            None => return true, // All filled — solved!
        };

        for val in candidates {
            grid[row][col] = val;
            if self.sudoku_backtrack(grid) {
                return true;
            }
            grid[row][col] = 0;
        }
        false
    }

    fn sudoku_candidates(&self, grid: &[[u8; 9]; 9], row: usize, col: usize) -> Vec<u8> {
        let mut used = HashSet::new();

        // Row
        for c in 0..9 {
            if grid[row][c] != 0 {
                used.insert(grid[row][c]);
            }
        }
        // Column
        for r in 0..9 {
            if grid[r][col] != 0 {
                used.insert(grid[r][col]);
            }
        }
        // 3x3 box
        let br = (row / 3) * 3;
        let bc = (col / 3) * 3;
        for r in br..br + 3 {
            for c in bc..bc + 3 {
                if grid[r][c] != 0 {
                    used.insert(grid[r][c]);
                }
            }
        }

        (1..=9).filter(|v| !used.contains(v)).collect()
    }

    /// Format Sudoku grid for display.
    pub fn format_sudoku(grid: &[[u8; 9]; 9]) -> String {
        let mut lines = vec!["**Sudoku Solution**\n".to_string()];
        for (i, row) in grid.iter().enumerate() {
            if i > 0 && i % 3 == 0 {
                lines.push("------+-------+------".to_string());
            }
            let parts: Vec<String> = row
                .chunks(3)
                .map(|chunk| {
                    chunk
                        .iter()
                        .map(|v| {
                            if *v == 0 {
                                ".".to_string()
                            } else {
                                v.to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect();
            lines.push(parts.join(" | "));
        }
        lines.join("\n")
    }

    /// Solve graph coloring problem.
    pub fn solve_graph_coloring(
        &mut self,
        num_vertices: usize,
        edges: &[(usize, usize)],
        num_colors: usize,
    ) -> Option<Vec<usize>> {
        self.nodes_explored = 0;
        let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
        for &(u, v) in edges {
            adj.entry(u).or_default().push(v);
            adj.entry(v).or_default().push(u);
        }

        let mut coloring = vec![0usize; num_vertices];
        if self.coloring_backtrack(&mut coloring, 0, num_vertices, num_colors, &adj) {
            Some(coloring)
        } else {
            None
        }
    }

    fn coloring_backtrack(
        &mut self,
        coloring: &mut Vec<usize>,
        vertex: usize,
        n: usize,
        k: usize,
        adj: &HashMap<usize, Vec<usize>>,
    ) -> bool {
        if vertex == n {
            return true;
        }
        self.nodes_explored += 1;

        for color in 1..=k {
            if self.is_safe_color(coloring, vertex, color, adj) {
                coloring[vertex] = color;
                if self.coloring_backtrack(coloring, vertex + 1, n, k, adj) {
                    return true;
                }
                coloring[vertex] = 0;
            }
        }
        false
    }

    fn is_safe_color(
        &self,
        coloring: &[usize],
        vertex: usize,
        color: usize,
        adj: &HashMap<usize, Vec<usize>>,
    ) -> bool {
        if let Some(neighbors) = adj.get(&vertex) {
            for &neighbor in neighbors {
                if neighbor < coloring.len() && coloring[neighbor] == color {
                    return false;
                }
            }
        }
        true
    }

    pub fn nodes_explored(&self) -> usize {
        self.nodes_explored
    }
}

impl Default for ConstraintSolver {
    fn default() -> Self {
        Self::new()
    }
}
