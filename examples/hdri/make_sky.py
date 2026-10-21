import math, struct, sys
W, H = 256, 128
def rgbe(r, g, b):
    m = max(r, g, b)
    if m < 1e-32: return bytes(4)
    e = math.frexp(m)[1]
    f = 256.0 / (2.0 ** e)
    return bytes([int(r*f), int(g*f), int(b*f), e + 128])
def lerp(a, b, t): return [a[i] + (b[i]-a[i])*t for i in range(3)]
zen, hor, gnd = [0.18, 0.34, 0.85], [1.1, 1.0, 0.9], [0.16, 0.14, 0.12]
sun_el, sun_az = math.radians(35), math.radians(-40)
sd = (math.cos(sun_el)*math.sin(sun_az), math.sin(sun_el), -math.cos(sun_el)*math.cos(sun_az))
out = bytearray(b"#?RADIANCE\n# A sky made by xms-imago's examples: blue zenith, warm horizon, a sun\nFORMAT=32-bit_rle_rgbe\n\n-Y %d +X %d\n" % (H, W))
for j in range(H):
    el = math.pi/2 - (j + 0.5) / H * math.pi
    for i in range(W):
        az = (i + 0.5) / W * 2*math.pi - math.pi
        d = (math.cos(el)*math.sin(az), math.sin(el), -math.cos(el)*math.cos(az))
        if el >= 0: c = lerp(hor, zen, min(1, (el/(math.pi/2))**0.6))
        else: c = lerp(hor, gnd, min(1, (-el/0.15)))
        cosang = sum(d[k]*sd[k] for k in range(3))
        ang = math.degrees(math.acos(max(-1, min(1, cosang))))
        glow = 0.6*math.exp(-ang/12)
        sun = 40.0 if ang < 2.5 else 0.0
        c = [c[0] + glow*1.0 + sun, c[1] + glow*0.85 + sun*0.95, c[2] + glow*0.6 + sun*0.85]
        out += rgbe(*c)
open(sys.argv[1], "wb").write(out)
