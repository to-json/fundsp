//! Various filters.

use super::audionode::*;
use super::math::*;
use super::signal::*;
use super::*;
use num_complex::Complex64;
use numeric_array::typenum::*;
use numeric_array::*;

#[derive(Copy, Clone, Debug, Default)]
pub struct BiquadCoefs<F> {
    pub a1: F,
    pub a2: F,
    pub b0: F,
    pub b1: F,
    pub b2: F,
}

impl<F: Real> BiquadCoefs<F> {
    /// Returns settings for a Butterworth lowpass filter.
    /// Cutoff is the -3 dB point of the filter in Hz.
    pub fn butter_lowpass(sample_rate: F, cutoff: F) -> Self {
        let c = F::from_f64;
        let f: F = tan(cutoff * c(PI) / sample_rate);
        let a0r: F = c(1.0) / (c(1.0) + c(SQRT_2) * f + f * f);
        let a1: F = (c(2.0) * f * f - c(2.0)) * a0r;
        let a2: F = (c(1.0) - c(SQRT_2) * f + f * f) * a0r;
        let b0: F = f * f * a0r;
        let b1: F = c(2.0) * b0;
        let b2: F = b0;
        Self { a1, a2, b0, b1, b2 }
    }

    /// Returns settings for a constant-gain bandpass resonator.
    /// The center frequency is given in Hz.
    /// Bandwidth is the difference in Hz between -3 dB points of the filter response.
    /// The overall gain of the filter is independent of bandwidth.
    pub fn resonator(sample_rate: F, center: F, bandwidth: F) -> Self {
        let c = F::from_f64;
        let r: F = exp(c(-PI) * bandwidth / sample_rate);
        let a1: F = c(-2.0) * r * cos(c(TAU) * center / sample_rate);
        let a2: F = r * r;
        let b0: F = sqrt(c(1.0) - r * r) * c(0.5);
        let b1: F = c(0.0);
        let b2: F = -b0;
        Self { a1, a2, b0, b1, b2 }
    }

    /// Arbitrary biquad.
    pub fn arbitrary(a1: F, a2: F, b0: F, b1: F, b2: F) -> Self {
        Self { a1, a2, b0, b1, b2 }
    }

    /// Frequency response at frequency `omega` expressed as fraction of sampling rate.
    pub fn response(&self, omega: f64) -> Complex64 {
        let z1 = Complex64::from_polar(1.0, -TAU * omega);
        let z2 = z1 * z1;
        /// Complex64 with real component `x` and imaginary component zero.
        fn re<T: Float>(x: T) -> Complex64 {
            Complex64::new(x.to_f64(), 0.0)
        }
        (re(self.b0) + re(self.b1) * z1 + re(self.b2) * z2)
            / (re(1.0) + re(self.a1) * z1 + re(self.a2) * z2)
    }
}

/// 2nd order IIR filter implemented in normalized Direct Form I.
/// Setting: coefficients as tuple (a1, a2, b0, b1, b2).
/// - Input 0: input signal.
/// - Output 0: filtered signal.
#[derive(Default, Clone)]
pub struct Biquad<T, F> {
    _marker: std::marker::PhantomData<T>,
    coefs: BiquadCoefs<F>,
    x1: F,
    x2: F,
    y1: F,
    y2: F,
    sample_rate: f64,
}

impl<T: Float, F: Real> Biquad<T, F> {
    pub fn new() -> Self {
        Self {
            sample_rate: DEFAULT_SR,
            ..Default::default()
        }
    }
    pub fn with_coefs(coefs: BiquadCoefs<F>) -> Self {
        Self {
            coefs,
            sample_rate: DEFAULT_SR,
            ..Default::default()
        }
    }
    pub fn coefs(&self) -> &BiquadCoefs<F> {
        &self.coefs
    }
    pub fn set_coefs(&mut self, coefs: BiquadCoefs<F>) {
        self.coefs = coefs;
    }
}

impl<T: Float, F: Real> AudioNode for Biquad<T, F> {
    const ID: u64 = 15;
    type Sample = T;
    type Inputs = typenum::U1;
    type Outputs = typenum::U1;
    type Setting = (F, F, F, F, F);

    fn set(&mut self, (a1, a2, b0, b1, b2): Self::Setting) {
        self.set_coefs(BiquadCoefs::arbitrary(a1, a2, b0, b1, b2));
    }

    fn reset(&mut self) {
        self.x1 = F::zero();
        self.x2 = F::zero();
        self.y1 = F::zero();
        self.y2 = F::zero();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    #[inline]
    fn tick(
        &mut self,
        input: &Frame<Self::Sample, Self::Inputs>,
    ) -> Frame<Self::Sample, Self::Outputs> {
        let x0 = convert(input[0]);
        let y0 = self.coefs.b0 * x0 + self.coefs.b1 * self.x1 + self.coefs.b2 * self.x2
            - self.coefs.a1 * self.y1
            - self.coefs.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x0;
        self.y2 = self.y1;
        self.y1 = y0;
        [convert(y0)].into()

        // Transposed Direct Form II would be:
        //   y0 = b0 * x0 + s1
        //   s1 = s2 + b1 * x0 - a1 * y0
        //   s2 = b2 * x0 - a2 * y0
    }

    fn route(&mut self, input: &SignalFrame, frequency: f64) -> SignalFrame {
        let mut output = new_signal_frame(self.outputs());
        output[0] = input[0].filter(0.0, |r| {
            r * self.coefs().response(frequency / self.sample_rate)
        });
        output
    }
}

/// Butterworth lowpass filter.
/// Setting: cutoff.
/// Number of inputs is `N`, either `U1` or `U2`.
/// - Input 0: input signal
/// - Input 1 (optional): cutoff frequency (Hz)
/// - Output 0: filtered signal
#[derive(Clone)]
pub struct ButterLowpass<T: Float, F: Real, N: Size<T>> {
    _marker: std::marker::PhantomData<N>,
    biquad: Biquad<T, F>,
    sample_rate: F,
    cutoff: F,
}

impl<T: Float, F: Real, N: Size<T>> ButterLowpass<T, F, N> {
    /// Create new Butterworth lowpass filter with initial `cutoff` frequency in Hz.
    pub fn new(cutoff: F) -> Self {
        let mut node = ButterLowpass {
            _marker: std::marker::PhantomData::default(),
            biquad: Biquad::new(),
            sample_rate: F::from_f64(DEFAULT_SR),
            cutoff: F::zero(),
        };
        node.biquad.reset();
        node.set_cutoff(cutoff);
        node
    }
    pub fn set_cutoff(&mut self, cutoff: F) {
        self.biquad
            .set_coefs(BiquadCoefs::butter_lowpass(self.sample_rate, cutoff));
        self.cutoff = cutoff;
    }
}

impl<T: Float, F: Real, N: Size<T>> AudioNode for ButterLowpass<T, F, N> {
    const ID: u64 = 16;
    type Sample = T;
    type Inputs = N;
    type Outputs = typenum::U1;
    type Setting = F;

    fn set(&mut self, setting: F) {
        self.set_cutoff(setting);
    }

    fn reset(&mut self) {
        self.biquad.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = convert(sample_rate);
        self.biquad.set_sample_rate(sample_rate);
        self.set_cutoff(self.cutoff);
    }

    #[inline]
    fn tick(
        &mut self,
        input: &Frame<Self::Sample, Self::Inputs>,
    ) -> Frame<Self::Sample, Self::Outputs> {
        if N::USIZE > 1 {
            let cutoff: F = convert(input[1]);
            if cutoff != self.cutoff {
                self.set_cutoff(cutoff);
            }
        }
        self.biquad.tick(&[input[0]].into())
    }

    fn route(&mut self, input: &SignalFrame, frequency: f64) -> SignalFrame {
        let mut output = new_signal_frame(self.outputs());
        output[0] = input[0].filter(0.0, |r| {
            r * self
                .biquad
                .coefs()
                .response(frequency / self.sample_rate.to_f64())
        });
        output
    }
}

/// Constant-gain bandpass filter (resonator).
/// Filter gain is (nearly) independent of bandwidth.
/// Setting: (center, bandwidth).
/// Number of inputs is `N`, either `U1` or `U3`.
/// - Input 0: input signal
/// - Input 1 (optional): filter center frequency (peak) (Hz)
/// - Input 2 (optional): filter bandwidth (distance) between -3 dB points (Hz)
/// - Output 0: filtered signal
#[derive(Clone)]
pub struct Resonator<T: Float, F: Real, N: Size<T>> {
    _marker: std::marker::PhantomData<N>,
    biquad: Biquad<T, F>,
    sample_rate: F,
    center: F,
    bandwidth: F,
}

impl<T: Float, F: Real, N: Size<T>> Resonator<T, F, N> {
    /// Create new resonator bandpass. Initial `center` frequency and `bandwidth` are specified in Hz.
    pub fn new(center: F, bandwidth: F) -> Self {
        let mut node = Resonator {
            _marker: std::marker::PhantomData::default(),
            biquad: Biquad::new(),
            sample_rate: F::from_f64(DEFAULT_SR),
            center,
            bandwidth,
        };
        node.biquad.reset();
        node.set_center_bandwidth(center, bandwidth);
        node
    }
    pub fn set_center_bandwidth(&mut self, center: F, bandwidth: F) {
        self.biquad
            .set_coefs(BiquadCoefs::resonator(self.sample_rate, center, bandwidth));
        self.center = center;
        self.bandwidth = bandwidth;
    }
}

impl<T: Float, F: Real, N: Size<T>> AudioNode for Resonator<T, F, N> {
    const ID: u64 = 17;
    type Sample = T;
    type Inputs = N;
    type Outputs = typenum::U1;
    type Setting = (F, F);

    fn set(&mut self, (center, bandwidth): Self::Setting) {
        self.set_center_bandwidth(center, bandwidth);
    }

    fn reset(&mut self) {
        self.biquad.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = convert(sample_rate);
        self.set_center_bandwidth(self.center, self.bandwidth);
    }

    #[inline]
    fn tick(
        &mut self,
        input: &Frame<Self::Sample, Self::Inputs>,
    ) -> Frame<Self::Sample, Self::Outputs> {
        if N::USIZE >= 3 {
            let center: F = convert(input[1]);
            let bandwidth: F = convert(input[2]);
            if center != self.center || bandwidth != self.bandwidth {
                self.biquad
                    .set_coefs(BiquadCoefs::resonator(self.sample_rate, center, bandwidth));
                self.center = center;
                self.bandwidth = bandwidth;
            }
        }
        self.biquad.tick(&[input[0]].into())
    }

    fn route(&mut self, input: &SignalFrame, frequency: f64) -> SignalFrame {
        let mut output = new_signal_frame(self.outputs());
        output[0] = input[0].filter(0.0, |r| {
            r * self
                .biquad
                .coefs()
                .response(frequency / self.sample_rate.to_f64())
        });
        output
    }
}

/// One-pole lowpass filter.
/// Setting: cutoff.
/// The number of inputs is `N`, either `U1` or `U2`.
/// - Input 0: input signal
/// - Input 1 (optional): cutoff frequency (Hz)
/// - Output 0: filtered signal
#[derive(Default, Clone)]
pub struct Lowpole<T: Float, F: Real, N: Size<T>> {
    _marker: std::marker::PhantomData<(T, N)>,
    value: F,
    coeff: F,
    cutoff: F,
    sample_rate: F,
}

impl<T: Float, F: Real, N: Size<T>> Lowpole<T, F, N> {
    /// Create new lowpass filter. Cutoff frequency is specified in Hz.
    pub fn new(cutoff: F) -> Self {
        let mut node = Lowpole {
            _marker: std::marker::PhantomData::default(),
            value: F::zero(),
            coeff: F::zero(),
            cutoff,
            sample_rate: convert(DEFAULT_SR),
        };
        node.set_cutoff(cutoff);
        node
    }

    /// Set the cutoff frequency (in Hz).
    /// This has no effect if the filter has a cutoff frequency input.
    pub fn set_cutoff(&mut self, cutoff: F) {
        self.cutoff = cutoff;
        self.coeff = exp(F::from_f64(-TAU) * cutoff / self.sample_rate);
    }
}

impl<T: Float, F: Real, N: Size<T>> AudioNode for Lowpole<T, F, N> {
    const ID: u64 = 18;
    type Sample = T;
    type Inputs = N;
    type Outputs = typenum::U1;
    type Setting = F;

    fn set(&mut self, setting: Self::Setting) {
        self.set_cutoff(setting);
    }

    fn reset(&mut self) {
        self.value = F::zero();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = convert(sample_rate);
        self.set_cutoff(self.cutoff);
    }

    #[inline]
    fn tick(
        &mut self,
        input: &Frame<Self::Sample, Self::Inputs>,
    ) -> Frame<Self::Sample, Self::Outputs> {
        if N::USIZE > 1 {
            let cutoff: F = convert(input[1]);
            if cutoff != self.cutoff {
                self.set_cutoff(cutoff);
            }
        }
        let x = convert(input[0]);
        self.value = (F::one() - self.coeff) * x + self.coeff * self.value;
        [convert(self.value)].into()
    }

    fn route(&mut self, input: &SignalFrame, frequency: f64) -> SignalFrame {
        let mut output = new_signal_frame(self.outputs());
        output[0] = input[0].filter(0.0, |r| {
            let c = self.coeff.to_f64();
            let f = frequency * TAU / self.sample_rate.to_f64();
            let z1 = Complex64::from_polar(1.0, -f);
            r * ((1.0 - c) / (1.0 - c * z1))
        });
        output
    }
}

/// DC blocking filter with cutoff frequency in Hz.
/// Setting: cutoff.
/// - Input 0: signal
/// - Output 0: zero centered signal
#[derive(Default, Clone)]
pub struct DCBlock<T: Float, F: Real> {
    _marker: std::marker::PhantomData<T>,
    x1: F,
    y1: F,
    cutoff: F,
    coeff: F,
    sample_rate: F,
}

impl<T: Float, F: Real> DCBlock<T, F> {
    /// Create new DC blocking filter with `cutoff` frequency specified in Hz.
    pub fn new(cutoff: F) -> Self {
        let mut node = DCBlock::<T, F> {
            cutoff,
            ..Default::default()
        };
        node.reset();
        node.set_sample_rate(DEFAULT_SR);
        node
    }

    /// Set the cutoff frequency (in Hz).
    pub fn set_cutoff(&mut self, cutoff: F) {
        self.cutoff = cutoff;
        self.coeff = F::one() - F::from_f64(TAU) / self.sample_rate * cutoff;
    }
}

impl<T: Float, F: Real> AudioNode for DCBlock<T, F> {
    const ID: u64 = 22;
    type Sample = T;
    type Inputs = typenum::U1;
    type Outputs = typenum::U1;
    type Setting = F;

    fn set(&mut self, setting: Self::Setting) {
        self.set_cutoff(setting);
    }

    fn reset(&mut self) {
        self.x1 = F::zero();
        self.y1 = F::zero();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = convert(sample_rate);
        self.set_cutoff(self.cutoff);
    }

    #[inline]
    fn tick(
        &mut self,
        input: &Frame<Self::Sample, Self::Inputs>,
    ) -> Frame<Self::Sample, Self::Outputs> {
        let x = convert(input[0]);
        let y0 = x - self.x1 + self.coeff * self.y1;
        self.x1 = x;
        self.y1 = y0;
        [convert(y0)].into()
    }

    fn route(&mut self, input: &SignalFrame, frequency: f64) -> SignalFrame {
        let mut output = new_signal_frame(self.outputs());
        output[0] = input[0].filter(0.0, |r| {
            let c = self.coeff.to_f64();
            let f = frequency * TAU / self.sample_rate.to_f64();
            let z1 = Complex64::from_polar(1.0, -f);
            r * ((1.0 - z1) / (1.0 - c * z1))
        });
        output
    }
}

/// Pinking filter (3 dB/octave lowpass).
/// - Input 0: input signal
/// - Output 0: filtered signal
#[derive(Default, Clone)]
pub struct Pinkpass<T: Float, F: Float> {
    // Algorithm by Paul Kellett. +-0.05 dB accuracy above 9.2 Hz @ 44.1 kHz.
    b0: F,
    b1: F,
    b2: F,
    b3: F,
    b4: F,
    b5: F,
    b6: F,
    _marker: std::marker::PhantomData<T>,
    sample_rate: F,
}

impl<T: Float, F: Float> Pinkpass<T, F> {
    /// Create pinking filter.
    pub fn new() -> Self {
        Pinkpass::<T, F> {
            sample_rate: convert(DEFAULT_SR),
            ..Pinkpass::default()
        }
    }
}

impl<T: Float, F: Float> AudioNode for Pinkpass<T, F> {
    const ID: u64 = 26;
    type Sample = T;
    type Inputs = U1;
    type Outputs = U1;
    type Setting = ();

    fn reset(&mut self) {
        self.b0 = F::zero();
        self.b1 = F::zero();
        self.b2 = F::zero();
        self.b3 = F::zero();
        self.b4 = F::zero();
        self.b5 = F::zero();
        self.b6 = F::zero();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = convert(sample_rate);
    }

    #[inline]
    fn tick(
        &mut self,
        input: &Frame<Self::Sample, Self::Inputs>,
    ) -> Frame<Self::Sample, Self::Outputs> {
        let x: F = convert(input[0]);
        self.b0 = F::from_f64(0.99886) * self.b0 + x * F::from_f64(0.0555179);
        self.b1 = F::from_f64(0.99332) * self.b1 + x * F::from_f64(0.0750759);
        self.b2 = F::from_f64(0.96900) * self.b2 + x * F::from_f64(0.1538520);
        self.b3 = F::from_f64(0.86650) * self.b3 + x * F::from_f64(0.3104856);
        self.b4 = F::from_f64(0.55000) * self.b4 + x * F::from_f64(0.5329522);
        self.b5 = F::from_f64(-0.7616) * self.b5 - x * F::from_f64(0.0168980);
        let out = (self.b0
            + self.b1
            + self.b2
            + self.b3
            + self.b4
            + self.b5
            + self.b6
            + x * F::from_f64(0.5362))
            * F::from_f64(0.115830421);
        self.b6 = x * F::from_f64(0.115926);
        [convert(out)].into()
    }

    fn route(&mut self, input: &SignalFrame, frequency: f64) -> SignalFrame {
        let mut output = new_signal_frame(self.outputs());
        output[0] = input[0].filter(0.0, |r| {
            let f = frequency * TAU / self.sample_rate.to_f64();
            let z1 = Complex64::from_polar(1.0, -f);
            let pole0 = 0.0555179 / (1.0 - 0.99886 * z1);
            let pole1 = 0.0750759 / (1.0 - 0.99332 * z1);
            let pole2 = 0.1538520 / (1.0 - 0.96900 * z1);
            let pole3 = 0.3104856 / (1.0 - 0.86650 * z1);
            let pole4 = 0.5329522 / (1.0 - 0.55000 * z1);
            let pole5 = -0.016898 / (1.0 + 0.7616 * z1);
            r * (pole0 + pole1 + pole2 + pole3 + pole4 + pole5 + 0.115926 * z1 + 0.5362)
                * 0.115830421
        });
        output
    }
}

/// 1st order allpass filter.
/// Setting: delay.
/// The number of inputs is `N`, either `U1` or `U2`.
/// - Input 0: input signal
/// - Input 1 (optional): delay in samples (delay > 0)
/// - Output 0: filtered signal
#[derive(Default, Clone)]
pub struct Allpole<T: Float, F: Float, N: Size<T>> {
    _marker: std::marker::PhantomData<(T, N)>,
    eta: F,
    x1: F,
    y1: F,
    sample_rate: F,
}

impl<T: Float, F: Float, N: Size<T>> Allpole<T, F, N> {
    /// Create new allpass filter. Initial `delay` is specified in samples.
    pub fn new(delay: F) -> Self {
        assert!(delay > F::zero());
        let mut node = Allpole {
            _marker: std::marker::PhantomData::default(),
            eta: F::zero(),
            x1: F::zero(),
            y1: F::zero(),
            sample_rate: convert(DEFAULT_SR),
        };
        node.set_delay(delay);
        node
    }

    /// Set delay in samples.
    #[inline]
    pub fn set_delay(&mut self, delay: F) {
        self.eta = (F::one() - delay) / (F::one() + delay);
    }
}

impl<T: Float, F: Float, N: Size<T>> AudioNode for Allpole<T, F, N> {
    const ID: u64 = 46;
    type Sample = T;
    type Inputs = N;
    type Outputs = typenum::U1;
    type Setting = F;

    fn set(&mut self, setting: Self::Setting) {
        self.set_delay(setting);
    }

    fn reset(&mut self) {
        self.x1 = F::zero();
        self.y1 = F::zero();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = convert(sample_rate);
    }

    #[inline]
    fn tick(
        &mut self,
        input: &Frame<Self::Sample, Self::Inputs>,
    ) -> Frame<Self::Sample, Self::Outputs> {
        if N::USIZE > 1 {
            self.set_delay(convert(input[1]));
        }
        let x0 = convert(input[0]);
        let y0 = self.eta * (x0 - self.y1) + self.x1;
        self.x1 = x0;
        self.y1 = y0;
        [convert(y0)].into()
    }

    fn route(&mut self, input: &SignalFrame, frequency: f64) -> SignalFrame {
        let mut output = new_signal_frame(self.outputs());
        output[0] = input[0].filter(0.0, |r| {
            let eta = self.eta.to_f64();
            let z1 = Complex64::from_polar(1.0, -frequency * TAU / self.sample_rate.to_f64());
            r * (eta + z1) / (1.0 + eta * z1)
        });
        output
    }
}

/// One-pole, one-zero highpass filter.
/// Setting: cutoff.
/// The number of inputs is `N`, either `U1` or `U2`.
/// - Input 0: input signal
/// - Input 1 (optional): cutoff frequency (Hz)
/// - Output 0: filtered signal
#[derive(Default, Clone)]
pub struct Highpole<T: Float, F: Real, N: Size<T>> {
    _marker: std::marker::PhantomData<(T, N)>,
    x1: F,
    y1: F,
    coeff: F,
    cutoff: F,
    sample_rate: F,
}

impl<T: Float, F: Real, N: Size<T>> Highpole<T, F, N> {
    /// Create new highpass filter. Initial `cutoff` frequency is specified in Hz.
    pub fn new(cutoff: F) -> Self {
        let mut node = Highpole {
            _marker: std::marker::PhantomData::default(),
            x1: F::zero(),
            y1: F::zero(),
            coeff: F::zero(),
            cutoff,
            sample_rate: convert(DEFAULT_SR),
        };
        node.set_cutoff(cutoff);
        node
    }
    pub fn set_cutoff(&mut self, cutoff: F) {
        self.cutoff = cutoff;
        self.coeff = exp(F::from_f64(-TAU) * cutoff / self.sample_rate);
    }
}

impl<T: Float, F: Real, N: Size<T>> AudioNode for Highpole<T, F, N> {
    const ID: u64 = 47;
    type Sample = T;
    type Inputs = N;
    type Outputs = typenum::U1;
    type Setting = F;

    fn set(&mut self, setting: Self::Setting) {
        self.set_cutoff(setting);
    }

    fn reset(&mut self) {
        self.x1 = F::zero();
        self.y1 = F::zero();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = convert(sample_rate);
        self.set_cutoff(self.cutoff);
    }

    #[inline]
    fn tick(
        &mut self,
        input: &Frame<Self::Sample, Self::Inputs>,
    ) -> Frame<Self::Sample, Self::Outputs> {
        if N::USIZE > 1 {
            let cutoff: F = convert(input[1]);
            if cutoff != self.cutoff {
                self.set_cutoff(cutoff);
            }
        }
        let x0 = convert(input[0]);
        let y0 = self.coeff * (self.y1 + x0 - self.x1);
        self.x1 = x0;
        self.y1 = y0;
        [convert(y0)].into()
    }

    fn route(&mut self, input: &SignalFrame, frequency: f64) -> SignalFrame {
        let mut output = new_signal_frame(self.outputs());
        output[0] = input[0].filter(0.0, |r| {
            let c = self.coeff.to_f64();
            let f = frequency * TAU / self.sample_rate.to_f64();
            let z1 = Complex64::from_polar(1.0, -f);
            r * (c * (1.0 - z1) / (1.0 - c * z1))
        });
        output
    }
}
