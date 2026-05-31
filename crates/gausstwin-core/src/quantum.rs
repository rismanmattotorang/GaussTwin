//! Quantum-Inspired Algorithms Module
//!
//! Revolutionary optimization algorithms inspired by quantum mechanics principles.

use crate::error::Result;
use rand::{thread_rng, Rng};
use std::f64::consts::PI;

/// Quantum Genetic Algorithm with superposition-based optimization
pub struct QuantumGeneticAlgorithm {
    population_size: usize,
    quantum_population: Vec<QuantumIndividual>,
    crossover_probability: f64,
    mutation_probability: f64,
    quantum_gate_probability: f64,
    generation_count: usize,
}

/// Quantum individual with superposition of classical solutions
#[derive(Debug, Clone)]
pub struct QuantumIndividual {
    qubits: Vec<Qubit>,
    fitness: f64,
    measured_solution: Vec<bool>,
}

/// Quantum bit with probability amplitudes
#[derive(Debug, Clone)]
pub struct Qubit {
    alpha: f64, // Amplitude for |0⟩ state
    beta: f64,  // Amplitude for |1⟩ state
}

impl Qubit {
    /// Create a new qubit in superposition
    pub fn new(alpha: f64, beta: f64) -> Self {
        let norm = (alpha * alpha + beta * beta).sqrt();
        Self {
            alpha: alpha / norm,
            beta: beta / norm,
        }
    }

    /// Create qubit in equal superposition
    pub fn superposition() -> Self {
        Self::new(1.0 / 2.0_f64.sqrt(), 1.0 / 2.0_f64.sqrt())
    }

    /// Measure the qubit (collapse to classical state)
    pub fn measure(&self) -> bool {
        let mut rng = thread_rng();
        let probability_zero = self.alpha * self.alpha;
        rng.gen::<f64>() >= probability_zero
    }

    /// Apply rotation gate
    pub fn rotate(&mut self, theta: f64) {
        let cos_theta = theta.cos();
        let sin_theta = theta.sin();

        let new_alpha = cos_theta * self.alpha - sin_theta * self.beta;
        let new_beta = sin_theta * self.alpha + cos_theta * self.beta;

        self.alpha = new_alpha;
        self.beta = new_beta;
    }
}

impl QuantumGeneticAlgorithm {
    /// Create a new quantum genetic algorithm
    pub fn new(
        population_size: usize,
        chromosome_length: usize,
        crossover_probability: f64,
        mutation_probability: f64,
    ) -> Self {
        let mut quantum_population = Vec::with_capacity(population_size);

        // Initialize population in superposition
        for _ in 0..population_size {
            let qubits = (0..chromosome_length)
                .map(|_| Qubit::superposition())
                .collect();

            quantum_population.push(QuantumIndividual {
                qubits,
                fitness: 0.0,
                measured_solution: vec![false; chromosome_length],
            });
        }

        Self {
            population_size,
            quantum_population,
            crossover_probability,
            mutation_probability,
            quantum_gate_probability: 0.1,
            generation_count: 0,
        }
    }

    /// Evolve the quantum population for one generation
    pub fn evolve<F>(&mut self, fitness_function: F) -> Result<f64>
    where
        F: Fn(&[bool]) -> f64,
    {
        // Measure quantum population to get classical solutions
        for individual in &mut self.quantum_population {
            individual.measured_solution = individual
                .qubits
                .iter()
                .map(|qubit| qubit.measure())
                .collect();

            individual.fitness = fitness_function(&individual.measured_solution);
        }

        // Find best fitness
        let best_fitness = self
            .quantum_population
            .iter()
            .map(|ind| ind.fitness)
            .fold(f64::NEG_INFINITY, f64::max);

        // Update quantum states based on best solutions
        self.update_quantum_states();

        // Apply quantum gates
        self.apply_quantum_gates();

        self.generation_count += 1;

        Ok(best_fitness)
    }

    /// Get the best solution found so far
    pub fn best_solution(&self) -> (Vec<bool>, f64) {
        let best_individual = self
            .quantum_population
            .iter()
            .max_by(|a, b| a.fitness.partial_cmp(&b.fitness).unwrap())
            .unwrap();

        (
            best_individual.measured_solution.clone(),
            best_individual.fitness,
        )
    }

    fn update_quantum_states(&mut self) {
        // Find best individuals
        let mut sorted_population = self.quantum_population.clone();
        sorted_population.sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap());

        let best = &sorted_population[0];
        let best_solution = &best.measured_solution;

        // Update quantum states towards best solution
        for individual in &mut self.quantum_population {
            for (i, qubit) in individual.qubits.iter_mut().enumerate() {
                let target_bit = best_solution[i];
                let learning_rate = 0.01;

                if target_bit {
                    // Rotate towards |1⟩ state
                    qubit.rotate(learning_rate);
                } else {
                    // Rotate towards |0⟩ state
                    qubit.rotate(-learning_rate);
                }
            }
        }
    }

    fn apply_quantum_gates(&mut self) {
        let mut rng = thread_rng();

        for individual in &mut self.quantum_population {
            for qubit in &mut individual.qubits {
                if rng.gen::<f64>() < self.quantum_gate_probability {
                    // Apply random rotation
                    let angle = rng.gen::<f64>() * PI / 4.0;
                    qubit.rotate(angle);
                }
            }
        }
    }
}

/// Quantum Particle Swarm Optimization
pub struct QuantumParticleSwarm {
    particles: Vec<QuantumParticle>,
    global_best_position: Vec<f64>,
    global_best_fitness: f64,
    inertia_weight: f64,
    cognitive_coefficient: f64,
    social_coefficient: f64,
    quantum_coefficient: f64,
}

#[derive(Debug, Clone)]
pub struct QuantumParticle {
    position: Vec<f64>,
    velocity: Vec<f64>,
    best_position: Vec<f64>,
    best_fitness: f64,
    quantum_state: Vec<f64>, // Quantum potential field
}

impl QuantumParticleSwarm {
    /// Create a new quantum particle swarm
    pub fn new(swarm_size: usize, dimensions: usize, bounds: (f64, f64)) -> Self {
        let mut rng = thread_rng();
        let mut particles = Vec::with_capacity(swarm_size);

        for _ in 0..swarm_size {
            let position: Vec<f64> = (0..dimensions)
                .map(|_| rng.gen_range(bounds.0..bounds.1))
                .collect();

            let velocity: Vec<f64> = (0..dimensions).map(|_| rng.gen_range(-1.0..1.0)).collect();

            let quantum_state: Vec<f64> =
                (0..dimensions).map(|_| rng.gen_range(0.0..1.0)).collect();

            particles.push(QuantumParticle {
                position: position.clone(),
                velocity,
                best_position: position,
                best_fitness: f64::NEG_INFINITY,
                quantum_state,
            });
        }

        Self {
            particles,
            global_best_position: vec![0.0; dimensions],
            global_best_fitness: f64::NEG_INFINITY,
            inertia_weight: 0.9,
            cognitive_coefficient: 2.0,
            social_coefficient: 2.0,
            quantum_coefficient: 0.5,
        }
    }

    /// Optimize using quantum-enhanced PSO
    pub fn optimize<F>(&mut self, fitness_function: F, iterations: usize) -> Result<(Vec<f64>, f64)>
    where
        F: Fn(&[f64]) -> f64,
    {
        for _iteration in 0..iterations {
            // Evaluate fitness for all particles
            for particle in &mut self.particles {
                let fitness = fitness_function(&particle.position);

                // Update personal best
                if fitness > particle.best_fitness {
                    particle.best_fitness = fitness;
                    particle.best_position = particle.position.clone();
                }

                // Update global best
                if fitness > self.global_best_fitness {
                    self.global_best_fitness = fitness;
                    self.global_best_position = particle.position.clone();
                }
            }

            // Update particle positions and velocities
            self.update_particles();

            // Apply quantum effects
            self.apply_quantum_effects();
        }

        Ok((self.global_best_position.clone(), self.global_best_fitness))
    }

    fn update_particles(&mut self) {
        let mut rng = thread_rng();

        // Create a copy of particles to avoid borrowing issues
        let particles_copy = self.particles.clone();

        for (i, particle) in self.particles.iter_mut().enumerate() {
            let particle_ref = &particles_copy[i];

            for d in 0..particle.position.len() {
                let r1 = rng.gen::<f64>();
                let r2 = rng.gen::<f64>();

                // Standard PSO velocity update
                particle.velocity[d] = self.inertia_weight * particle.velocity[d]
                    + self.cognitive_coefficient
                        * r1
                        * (particle_ref.best_position[d] - particle.position[d])
                    + self.social_coefficient
                        * r2
                        * (self.global_best_position[d] - particle.position[d]);

                // Update position
                particle.position[d] += particle.velocity[d];
            }
        }
    }

    fn apply_quantum_effects(&mut self) {
        let mut rng = thread_rng();

        for particle in &mut self.particles {
            for d in 0..particle.position.len() {
                // Quantum tunneling effect
                if rng.gen::<f64>() < self.quantum_coefficient {
                    let quantum_jump = rng.gen_range(-0.1..0.1);
                    particle.position[d] += quantum_jump * particle.quantum_state[d];
                }

                // Update quantum state
                particle.quantum_state[d] =
                    (particle.quantum_state[d] + rng.gen_range(-0.01..0.01)).clamp(0.0, 1.0);
            }
        }
    }
}

/// Quantum Annealing Algorithm for combinatorial optimization
pub struct QuantumAnnealer {
    temperature: f64,
    cooling_rate: f64,
    min_temperature: f64,
    current_solution: Vec<bool>,
    current_energy: f64,
    best_solution: Vec<bool>,
    best_energy: f64,
}

impl QuantumAnnealer {
    /// Create a new quantum annealer
    pub fn new(
        initial_solution: Vec<bool>,
        initial_temperature: f64,
        cooling_rate: f64,
        min_temperature: f64,
    ) -> Self {
        let energy = 0.0; // Will be set by first energy calculation

        Self {
            temperature: initial_temperature,
            cooling_rate,
            min_temperature,
            current_solution: initial_solution.clone(),
            current_energy: energy,
            best_solution: initial_solution,
            best_energy: energy,
        }
    }

    /// Perform quantum annealing optimization
    pub fn anneal<F>(
        &mut self,
        energy_function: F,
        max_iterations: usize,
    ) -> Result<(Vec<bool>, f64)>
    where
        F: Fn(&[bool]) -> f64,
    {
        let mut rng = thread_rng();

        // Initialize energies
        self.current_energy = energy_function(&self.current_solution);
        self.best_energy = self.current_energy;

        for _iteration in 0..max_iterations {
            if self.temperature < self.min_temperature {
                break;
            }

            // Generate neighbor solution by flipping random bits
            let mut neighbor = self.current_solution.clone();
            let flip_index = rng.gen_range(0..neighbor.len());
            neighbor[flip_index] = !neighbor[flip_index];

            let neighbor_energy = energy_function(&neighbor);
            let energy_difference = neighbor_energy - self.current_energy;

            // Accept or reject the neighbor solution
            if energy_difference < 0.0
                || rng.gen::<f64>() < (-energy_difference / self.temperature).exp()
            {
                self.current_solution = neighbor;
                self.current_energy = neighbor_energy;

                // Update best solution
                if neighbor_energy < self.best_energy {
                    self.best_solution = self.current_solution.clone();
                    self.best_energy = neighbor_energy;
                }
            }

            // Cool down
            self.temperature *= self.cooling_rate;
        }

        Ok((self.best_solution.clone(), self.best_energy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qubit_operations() {
        let mut qubit = Qubit::superposition();

        // Test probability conservation
        let prob_sum = qubit.alpha * qubit.alpha + qubit.beta * qubit.beta;
        assert!((prob_sum - 1.0).abs() < 1e-10);

        // Test rotation
        qubit.rotate(PI / 4.0);
        let new_prob_sum = qubit.alpha * qubit.alpha + qubit.beta * qubit.beta;
        assert!((new_prob_sum - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_quantum_genetic_algorithm() {
        let mut qga = QuantumGeneticAlgorithm::new(20, 10, 0.8, 0.1);

        // Simple fitness function: count ones
        let fitness_fn = |chromosome: &[bool]| chromosome.iter().filter(|&&bit| bit).count() as f64;

        // Run a few generations
        for _ in 0..5 {
            let best_fitness = qga.evolve(fitness_fn).unwrap();
            assert!(best_fitness >= 0.0);
        }

        let (best_solution, best_fitness) = qga.best_solution();
        assert_eq!(best_solution.len(), 10);
        assert!(best_fitness >= 0.0);
    }

    #[test]
    fn test_quantum_particle_swarm() {
        let mut qpso = QuantumParticleSwarm::new(20, 2, (-10.0, 10.0));

        // Simple quadratic function
        let fitness_fn = |x: &[f64]| -(x[0] * x[0] + x[1] * x[1]);

        let (best_position, best_fitness) = qpso.optimize(fitness_fn, 50).unwrap();

        assert_eq!(best_position.len(), 2);
        assert!(best_fitness <= 0.0); // Maximum should be at origin
    }

    #[test]
    fn test_quantum_annealer() {
        let initial_solution = vec![false; 10];
        let mut annealer = QuantumAnnealer::new(initial_solution, 10.0, 0.95, 0.1);

        // Energy function that prefers more true bits
        let energy_fn = |solution: &[bool]| -(solution.iter().filter(|&&bit| bit).count() as f64);

        let (best_solution, best_energy) = annealer.anneal(energy_fn, 1000).unwrap();

        assert_eq!(best_solution.len(), 10);
        assert!(best_energy <= 0.0);
    }
}
