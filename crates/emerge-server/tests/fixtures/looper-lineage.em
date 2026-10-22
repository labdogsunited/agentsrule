// Fixture for the genome view: two writers of `memory`, one of them reading its own output, a signal
// nothing reads, a dormant gene, and a dormant module.
lineage Looper in Dish {
    profile fast {
        genes: 32;
        signals: 16;
        modules: 8;
    }

    seed genome {
        module main {
            sense inventory(s: body) -> fullness @inv;
            regulate weigh(bias: 0.0, curve: tanh) <- [fullness, memory] -> memory @feedback;
            regulate weigh(bias: 1.0, curve: step) <- [fullness] -> memory @second;
            regulate weigh(bias: 0.0, curve: tanh) <- [fullness] -> unused @sink;
            action move(speed: 1.0) @go;
            action turn(angle_deg: 30.0) <- memory threshold 0.0 @turn;
            reaction photosynthesis rate 1u @photo;
        }
        module metabolism {
            reaction respiration rate 1u @respire;
            action take(s: light, max: 1u) @eat_light;
        }
    }

    genesis {
        count: 50;
        position = uniform(x: 0.0..512.0, y: 0.0..512.0);
        transfer(energy, from: spark, to: self, amount: 5u) @transfer_energy;
    }
}
