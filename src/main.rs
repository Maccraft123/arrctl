use anyhow::{bail, Result};
use bitfield::bitfield;
use clap::Parser;
use msru::{Accessor, Msr};
use raw_cpuid::CpuId;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    get_tdp: bool,

    #[arg(long)]
    get_tdc: bool,

    #[arg(long)]
    get_tjmax: bool,

    #[arg(long)]
    get_turbo_ratios: bool,

    #[arg(long, value_name = "WATTS")]
    set_tdp: Option<u64>,

    #[arg(long, value_name = "AMPS")]
    set_tdc: Option<u64>,
}

fn ensure_cpu_good() {
    let cpuid = CpuId::new();
    let vf = cpuid
        .get_vendor_info()
        .expect("Failed getting cpuid vendor info");
    if vf.as_str() != "GenuineIntel" {
        panic!("Only Intel CPUs are supported");
    }

    let fi = cpuid
        .get_feature_info()
        .expect("Failed to get feature information cpuid leaf");
    if fi.extended_family_id() != 0x0 || fi.family_id() != 0x6 || fi.model_id() != 0x25 {
        panic!("Only Arrandale CPUs are supported!");
    }
}

const MSR_PLATFORM_INFO: u32 = 0xce;
const IA32_MISC_ENABLE: u32 = 0x1a0;
const MSR_TEMPERATURE_TARGET: u32 = 0x1a2;
const MSR_TURBO_LIMITS: u32 = 0x1ac;
const MSR_TURBO_RATIOS: u32 = 0x1ad;

bitfield! {
    pub struct MsrPlatformInfo(u64);

    max_non_turbo_ratio, _: 15, 8;
    programmable_turbo_ratio, _: 28;
    programmable_tdc_tdp, _: 29;
    minimum_ratio, _: 47, 40;
}

bitfield! {
    pub struct Ia32MiscEnable(u64);

    // INCOMPLETE
    turbo_disable, set_turbo_disable: 38;
}

bitfield! {
    pub struct MsrTemperatureTarget(u64);

    get, _: 23, 16;
}

bitfield! {
    pub struct MsrTurboLimits(u64);

    tdp, set_tdp: 14, 0;
    tdp_override, set_tdp_override: 15;
    tdc, set_tdc: 30, 16;
    tdc_override, set_tdc_override: 31;
}

bitfield! {
    pub struct MsrTurboRatios(u64);

    one_core, _: 7, 0;
    two_cores, _: 15, 8;
    three_cores, _: 23, 16;
    four_cores, _: 31, 24;
}

fn ia32_misc_enable() -> Ia32MiscEnable {
    Ia32MiscEnable(rdmsr(IA32_MISC_ENABLE, 0))
}

fn msr_platform_info() -> MsrPlatformInfo {
    MsrPlatformInfo(rdmsr(MSR_PLATFORM_INFO, 0))
}

fn msr_temperature_target() -> MsrTemperatureTarget {
    MsrTemperatureTarget(rdmsr(MSR_TEMPERATURE_TARGET, 0))
}

fn msr_turbo_limits() -> MsrTurboLimits {
    MsrTurboLimits(rdmsr(MSR_TURBO_LIMITS, 0))
}

fn msr_turbo_ratios() -> MsrTurboRatios {
    MsrTurboRatios(rdmsr(MSR_TURBO_RATIOS, 0))
}

fn rdmsr(which: u32, core: u16) -> u64 {
    Msr::new(which, core)
        .unwrap()
        .read()
        .unwrap()
}

fn wrmsr(which: u32, core: u16, val: u64) {
    let mut msr = Msr::new(which, core).unwrap();
    msr.set_value(val);
    msr.write().unwrap();
}

fn main() -> Result<()> {
    if unsafe { libc::geteuid() }  != 0 {
        bail!("You have to run this program as root");
    }
    let args = Cli::parse();
    ensure_cpu_good();

    let plat_info = msr_platform_info();

    if (args.get_tdp || args.get_tdc) && (args.set_tdp.is_some() || args.set_tdc.is_some()) {
        bail!("Can't set and get TDP or TDC values at the same time");
    }

    if (args.set_tdp.is_some() || args.set_tdc.is_some()) && !plat_info.programmable_tdc_tdp() {
        bail!("CPU doesn't support setting TDP and TDC");
    }

    if args.get_tdp || args.get_tdc || args.set_tdp.is_some() || args.set_tdc.is_some() {
        let mut turbo_limits = msr_turbo_limits();
        if args.get_tdp {
            println!("Maximum turbo TDP: {} W", turbo_limits.tdp() as f32 / 8.0);
            println!("Turbo TDP override status: {}", turbo_limits.tdp_override());
        }
        if args.get_tdc {
            println!("Maximum turbo TDC: {} A", turbo_limits.tdc() as f32 / 8.0);
            println!("Turbo TDP override status: {}", turbo_limits.tdp_override());
        }
        if let Some(tdp) = args.set_tdp {
            turbo_limits.set_tdp(tdp * 8);
            turbo_limits.set_tdp_override(true);
        }
        if let Some(tdc) = args.set_tdc {
            turbo_limits.set_tdc(tdc * 8);
            turbo_limits.set_tdc_override(true);
        }
        if args.set_tdp.is_some() || args.set_tdc.is_some() {
            wrmsr(MSR_TURBO_LIMITS, 0, turbo_limits.0);
        }
    }

    if args.get_tjmax {
        let tjmax = msr_temperature_target();
        println!("TJmax is {} celsius", tjmax.get());
    }

    if args.get_turbo_ratios {
        let turbo_ratios = msr_turbo_ratios();

        if turbo_ratios.one_core() != 0 {
            println!("Max turbo ratio for one core: {}", turbo_ratios.one_core());
        }
        if turbo_ratios.two_cores() != 0 {
            println!("Max turbo ratio for two cores: {}", turbo_ratios.two_cores());
        }
        if turbo_ratios.three_cores() != 0 {
            println!("Max turbo ratio for three cores: {}", turbo_ratios.three_cores());
        }
        if turbo_ratios.four_cores() != 0 {
            println!("Max turbo ratio for four cores: {}", turbo_ratios.four_cores());
        }
    }

    Ok(())
}
