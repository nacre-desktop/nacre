-- SPDX-License-Identifier: GPL-3.0-only
-- THE pearl recipe (Lacquer and inlay standard, Final.dc.html --pearl), drawn with Bead's radial fills (B0331).
-- Tokens, one elliptical layer each ({cx,cy,rx,ry} are fractions of the box; CSS "circle at x% y%" with its default
-- farthest-corner radius, stop offsets are CSS percentages of that radius):
--   pearl.body       the nacre ground: lilac-white centre falling to a violet-grey edge
--   pearl.rim_mint   the mint glow, low left (orientMint)
--   pearl.rim_rose   the rose glow, low right (orientRose)
--   pearl.highlight  the white highlight, upper left
-- pearl.layers(true) is the whole pearl, bottom first: body, rim_mint, rim_rose, highlight. pearl.layers(false) is the
-- highlight alone, for a pearl whose body is a flat accent fill. Pass either as `radial=` on a Box with a radius.
local ui=require("bead.ui")
local function white(a) return {255,255,255,a} end
local P={
    body={cx=.5,cy=.46,rx=.736,ry=.736,stops={{0,{241,235,240,255}},{.5,{230,222,231,255}},{.82,{201,191,208,255}},{1,{157,146,166,255}}}},
    rim_mint={cx=.2,cy=.76,rx=1.1,ry=1.1,stops={{0,ui.alpha("orientMint",.55)},{.38,ui.alpha("orientMint",0)}}},
    rim_rose={cx=.74,cy=.8,rx=1.09,ry=1.09,stops={{0,ui.alpha("orientRose",.85)},{.44,ui.alpha("orientRose",0)}}},
    highlight={cx=.33,cy=.27,rx=.99,ry=.99,stops={{0,white(255)},{.07,white(230)},{.26,white(0)}}},
}
local full={P.body,P.rim_mint,P.rim_rose,P.highlight}
local sheen={P.highlight}
function P.layers(body) return body and full or sheen end
return P
