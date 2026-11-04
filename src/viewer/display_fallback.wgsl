// The display transform when the config's cannot be made: linear Rec.709 to
// sRGB's curve, as the Viewer drew before colour management.

fn imago_display(inPixel: vec4<f32>) -> vec4<f32> {
    let c = max(inPixel.rgb, vec3<f32>(0.0));
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return vec4<f32>(select(hi, lo, c <= vec3<f32>(0.0031308)), inPixel.a);
}
