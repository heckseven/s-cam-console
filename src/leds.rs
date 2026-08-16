use dc34_api::*;

/// Standalone ring patterns, selected by `LedManagerOp::SetPattern`.
///
/// These are ordinary phenotypes installed as the gene and expressed, so they reuse the
/// existing renderer rather than adding a second lighting path. Index 0 is not a pattern: it restores gene
/// expression, which stays the default and is the badge's protected behaviour.
///
/// Field meanings, from `Haploid`: cd_* drive the brightness cycle (period is 0..=6, larger
/// is slower), sat is saturation, hue_base/hue_bound bound the colour range and must satisfy
/// bound >= base, hue_ratedir sets hue drift speed and direction, chaser sets the rotating
/// offset around the ring, and nonlin shapes the brightness curve.
const PATTERNS: [(&str, Haploid); 5] = [
    // A slow full-spectrum drift - calm, uses the whole ring evenly.
    ("rainbow", Haploid {
        cd_period: 5, cd_rate: 24, cd_dir: 1, sat: 255,
        hue_ratedir: 36, hue_base: 0, hue_bound: 255, chaser: 24, nonlin: 96,
    }),
    // A single bright point chasing quickly around the ring.
    ("chase", Haploid {
        cd_period: 1, cd_rate: 200, cd_dir: 1, sat: 255,
        hue_ratedir: 8, hue_base: 150, hue_bound: 170, chaser: 220, nonlin: 220,
    }),
    // Slow symmetric breathing in a narrow cyan band.
    ("breathe", Haploid {
        cd_period: 6, cd_rate: 12, cd_dir: 0, sat: 220,
        hue_ratedir: 0, hue_base: 120, hue_bound: 135, chaser: 0, nonlin: 64,
    }),
    // Warm amber ember, low saturation drift, barely moving.
    ("ember", Haploid {
        cd_period: 4, cd_rate: 40, cd_dir: 0, sat: 180,
        hue_ratedir: 4, hue_base: 10, hue_bound: 40, chaser: 8, nonlin: 160,
    }),
    // Fast, high-contrast, full-spectrum - deliberately loud.
    ("riot", Haploid {
        cd_period: 0, cd_rate: 255, cd_dir: 1, sat: 255,
        hue_ratedir: 255, hue_base: 0, hue_bound: 255, chaser: 255, nonlin: 255,
    }),
];

pub const PATTERN_COUNT: usize = PATTERNS.len();

/// Build a Diploid whose `phenotype()` comes out as `p`.
///
/// `phenotype()` blends two strands with dominance rules, so `Diploid([p, p])` does NOT
/// express as `p`: saturating adds pin sat and chaser to 255, and the hue_ratedir rule
/// collapses most inputs to the same value - which made four of the five patterns express
/// almost identically. Each field is inverted here so the pattern that was written is the
/// pattern that renders.
fn as_gene(p: Haploid) -> Diploid {
    let half = p.sat / 2;
    // hue_ratedir is (2 + (14 - min(a+b,14))) % 14; solve for the sum that yields p's value
    let want = p.hue_ratedir % 14;
    let sum = if want >= 2 { 16 - want } else { 16 - (want + 14) };
    let a = Haploid {
        cd_period: p.cd_period,
        cd_rate: p.cd_rate,
        cd_dir: p.cd_dir,
        sat: half,
        hue_ratedir: sum.min(14),
        hue_base: p.hue_base,
        hue_bound: p.hue_bound,
        chaser: p.chaser,
        // phenotype reads chaser from the FIRST strand for nonlin, so this strand's chaser
        // is what lands there; the second strand carries the remainder
        nonlin: 0,
    };
    let b = Haploid {
        cd_period: p.cd_period,
        cd_rate: p.cd_rate,
        cd_dir: 0,
        sat: p.sat - half,
        hue_ratedir: 0,
        hue_base: p.hue_base,
        hue_bound: p.hue_bound,
        chaser: 0,
        nonlin: p.nonlin.saturating_sub(p.chaser),
    };
    Diploid([a, b])
}

pub fn start_leds() {
    std::thread::spawn(move || {
        leds();
    });
}

fn leds() {
    let xns = xous_names::XousNames::new().unwrap();

    let sid = xns.register_name(dc34_api::LED_SERVER, None).unwrap();

    #[cfg(not(feature = "uber"))]
    const LED_COUNT: u8 = 10;
    #[cfg(feature = "uber")]
    const LED_COUNT: u8 = 18;

    let mut lightgenes =
        crate::bio::lightgenes::Lightgenes::new(arbitrary_int::u5::new(15), LED_COUNT, None).unwrap();

    let mut rate_param: u8 = 1;
    // The badge's own gene, put aside while a standalone pattern is showing so that
    // selecting "gene (default)" puts back exactly what was there before.
    let mut saved_gene: Option<Diploid> = None;
    let mut msg_opt = None;
    loop {
        xous::reply_and_receive_next(sid, &mut msg_opt).unwrap();
        let opcode = {
            let msg = msg_opt.as_mut().unwrap();
            num_traits::FromPrimitive::from_usize(msg.body.id()).unwrap_or(LedManagerOp::Invalid)
        };
        match opcode {
            LedManagerOp::Autogamy => {
                lightgenes.autogamy();
                lightgenes.express();
            }
            LedManagerOp::SetTestRate => {
                if let Some(scalar) = msg_opt.as_mut().unwrap().body.scalar_message_mut() {
                    rate_param = scalar.arg1.min(255) as u8;
                }
            }
            LedManagerOp::Syngamy => {
                if let Some(scalar) = msg_opt.as_mut().unwrap().body.scalar_message_mut() {
                    if let Some(sperm) = Haploid::deserialize_u32(&[
                        scalar.arg1 as u32,
                        scalar.arg2 as u32,
                        scalar.arg3 as u32,
                        scalar.arg4 as u32,
                    ]) {
                        // rewritten to match exactly what's happening in the production loop
                        let mut egg = lightgenes.meiosis().unwrap();
                        let rate = MutationRate::from_param(rate_param);
                        log::info!("mutate at rate {:?}: {:?}", rate, egg);
                        mutate(&mut egg, rate);
                        log::info!("Mutated {:?}", egg);
                        lightgenes.gene = Some(Diploid([egg, sperm]));
                        lightgenes.express();
                    } else {
                        log::warn!("Couldn't deserialize gene in call to Syngamy, ignoring")
                    }
                }
            }
            LedManagerOp::Force => {
                if let Some(scalar) = msg_opt.as_mut().unwrap().body.scalar_message_mut() {
                    if let Some(phenotype) = Haploid::deserialize_u32(&[
                        scalar.arg1 as u32,
                        scalar.arg2 as u32,
                        scalar.arg3 as u32,
                        scalar.arg4 as u32,
                    ]) {
                        lightgenes.force(phenotype);
                    } else {
                        log::warn!("Couldn't deserialize gene in call to Force, ignoring")
                    }
                }
            }
            LedManagerOp::GeneTest => {
                if let Some(scalar) = msg_opt.as_mut().unwrap().body.scalar_message_mut() {
                    let badge_type = BadgeType::try_from(scalar.arg1 as u8).unwrap_or(BadgeType::None);
                    lightgenes
                        .gene
                        .replace(Diploid([Haploid::from_type(&badge_type), Haploid::from_type(&badge_type)]));
                    log::info!("Init to {:?}: gene {:?}", badge_type, lightgenes.gene);
                    lightgenes.express();
                }
            }
            LedManagerOp::SetGene => {
                if let Some(mem) = msg_opt.as_ref().unwrap().body.memory_message() {
                    lightgenes.gene = Some(Diploid::receive(mem));
                    log::info!("Received gene {:?}", lightgenes.gene);
                    lightgenes.express();
                }
            }
            LedManagerOp::JackEyes => {
                if let Some(scalar) = msg_opt.as_ref().unwrap().body.scalar_message() {
                    lightgenes.jack_eyes(scalar.arg1 != 0);
                }
            }
            LedManagerOp::SetPattern => {
                if let Some(scalar) = msg_opt.as_mut().unwrap().body.scalar_message_mut() {
                    let sel = scalar.arg1;
                    // Install the pattern AS the gene and express it, exactly the way GeneTest
                    // does. force() writes straight to the engine and left the ring showing
                    // whatever gene expression had already set; express() is the path that
                    // demonstrably drives the ring, so patterns use it too.
                    let status;
                    if sel == 0 {
                        if let Some(gene) = saved_gene.take() {
                            lightgenes.gene = Some(gene);
                        }
                        lightgenes.express();
                        log::info!("LED pattern cleared, gene expression restored");
                        status = if lightgenes.gene.is_some() { 1 } else { 2 };
                    } else if let Some((name, phenotype)) = PATTERNS.get(sel - 1) {
                        if saved_gene.is_none() {
                            saved_gene = lightgenes.gene.clone();
                        }
                        lightgenes.gene = Some(as_gene(*phenotype));
                        lightgenes.express();
                        log::info!("LED pattern {} ({}) selected", sel, name);
                        status = 3;
                    } else {
                        log::warn!("LED pattern {} out of range, ignoring", sel);
                        status = 4;
                    }
                    let _ = status;
                }
            }
            LedManagerOp::Pause => {
                if let Some(scalar) = msg_opt.as_mut().unwrap().body.scalar_message_mut() {
                    lightgenes.pause_rendering(scalar.arg1 != 0);
                }
            }
            LedManagerOp::Invalid => {
                log::error!("Invalid LED manager operation: {:?}", opcode);
            }
        };
    }
}
