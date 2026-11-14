-- SPDX-License-Identifier: GPL-3.0-only
-- Media (design review D9): an art-led player card (secondary tint, the album art blurred as the
-- backdrop, 120 px art, 20/600 title, 44 px primary pause), the lyrics card only while lyrics
-- exist, and the shared empty state when nothing is playing. Timed lyric cues and shuffle/repeat
-- await published player fields.
local ui=require("bead.ui")
local kit=require("bead.kit")
local empty=require("../../ui/components/empty-state")
local T=kit.tokens
local function rounded_clip(w,h,r)
    local points={}
    for corner,center in ipairs{{w-r,r},{w-r,h-r},{r,h-r},{r,r}} do
        for step=0,7 do
            local angle=(corner-2)*math.pi/2+step*math.pi/14
            points[#points+1]={math.clamp((center[1]+r*math.cos(angle))/w,0,1),math.clamp((center[2]+r*math.sin(angle))/h,0,1)}
        end
    end
    return points
end
return function()
    local get=function(key) return ui.resolve_for_check(ui.bind(key)) end
    local state,control,art,progress=get("dashboard.state"),get("dashboard.control"),get("dashboard.art"),get("dashboard.progress")
    local rows,player=get("dashboard.rows"),get("dashboard.player_identity")
    local op=function(name,key) return control and get(key) and ui.action("dashboard.do",name) or nil end
    local W,H,PAD,GUTTER,RADIUS=940,562,T.spacing.xxl,T.spacing.lg,T.spacing.xl
    local BODY,CAPTION=T.text.body*4/3,T.text.caption*4/3
    local hasLyrics=player~="" and (#rows>0 or get("dashboard.detail")~="")
    local cardW=hasLyrics and math.floor((W-GUTTER)*7/12) or W
    local inner=cardW-2*PAD
    local function iconButton(id,glyph,label,action,size,primary)
        return kit.IconButton{id=id,text=glyph,aria_label=label,width=size,height=size,padding=0,radius=T.radii.full,
            font_size=(primary and T.icons.large or T.icons.medium)*4/3,fill=primary and "primary" or "transparent",
            hover_fill=primary and ui.tint("primary","onSurface",.08) or ui.alpha("onSurface",.08),on_activate=action,
            color=primary and "onPrimary" or (action and "onSurface" or "onSurfaceVariant")}
    end
    -- a flat secondary tint of the container (kit.Fill's stepped blend bands over a card this wide)
    local backdrop={ui.Box{id="tint",position="absolute",left=0,top=0,width=cardW,height=H,radius=RADIUS,fill=ui.tint("surfaceContainer","secondary",.2)}}
    if art then
        -- the blurred art is the backdrop (an October owner request); the overlay keeps text readable
        backdrop[#backdrop+1]=kit.BlurredArtwork{id="backdrop",position="absolute",left=0,top=0,width=cardW,height=H,icon="player-art",
            image_fit="cover",clip_polygon=rounded_clip(cardW,H,RADIUS)}
        backdrop[#backdrop+1]=ui.Box{id="scrim",position="absolute",left=0,top=0,width=cardW,height=H,radius=RADIUS,fill=ui.alpha("surfaceContainer",.72)}
    end
    local content
    if player=="" then
        content={empty{top=0,width=cardW,height=H,glyph="music_note",title=ui.tr("bead.dashboard.media-empty"),
            detail=ui.tr("bead.dashboard.playback-unavailable-detail")}}
    else
        local ART=120
        local textW=inner-ART-T.spacing.xl
        local clock=get("dashboard.metric")
        local position,duration=string.match(clock,"^(.-) / (.+)$")
        local artwork=ui.Box{id="artwork",width=ART,height=ART,radius=T.spacing.lg,fill="surfaceContainerHigh",shrink=0,
            art and ui.Image{position="absolute",left=0,top=0,icon="player-art",width=ART,height=ART,image_fit="cover",
                clip_polygon=rounded_clip(ART,ART,T.spacing.lg),aria_label=ui.tr("bead.dashboard.media-art")}
                or ui.Text{position="absolute",left=(ART-48)/2,top=(ART-48)/2,width=48,height=48,text="music_note",
                    family=T.fonts.icon,font_size=48,text_align="center",color="primary"}}
        local transport=ui.Row{id="transport",width=textW,height=44,gap=T.spacing.compact,align="center",
            iconButton("seek-back","replay_10",ui.tr("bead.dashboard.media-seek-back"),op("player.seek:-10","dashboard.can_seek"),36),
            iconButton("previous","skip_previous",ui.tr("bead.dashboard.media-previous"),op("player.previous","dashboard.can_previous"),36),
            iconButton("toggle",state=="playing" and "pause" or "play_arrow",
                state=="playing" and ui.tr("bead.dashboard.pause") or ui.tr("bead.dashboard.media-play"),
                op("player.play_pause","dashboard.can_toggle"),44,true),
            iconButton("next","skip_next",ui.tr("bead.dashboard.media-next"),op("player.next","dashboard.can_next"),36),
            iconButton("seek-forward","forward_10",ui.tr("bead.dashboard.media-seek-forward"),op("player.seek:10","dashboard.can_seek"),36)}
        local metadata=ui.Column{id="metadata",width=textW,height=ART,gap=T.spacing.xs,justify="center",
            ui.Text{id="player",text=player,width=textW,height=16,font_size=CAPTION,color="onSurfaceVariant"},
            ui.Text{id="title",text=ui.bind("dashboard.track"),width=textW,height=26,font_size=20,font_weight=600,color="onSurface"},
            ui.Text{id="description",text=ui.bind("dashboard.track_detail"),width=textW,height=20,font_size=BODY,color="onSurfaceVariant"},
            ui.Box{id="spacer",width=textW,height=T.spacing.xs},
            transport}
        local blockH=ART+T.spacing.xl+T.spacing.sm+T.spacing.xl+18
        content={ui.Column{id="player",position="absolute",left=PAD,top=(H-blockH)/2,width=inner,gap=T.spacing.xl,
            ui.Row{id="now-playing",width=inner,height=ART,gap=T.spacing.xl,align="center",artwork,metadata},
            kit.LevelMeter{id="seek",value=progress,width=inner,height=T.spacing.sm,track="surfaceContainerHigh",ink="primary"},
            ui.Row{id="clock",width=inner,height=18,justify="space-between",
                ui.Text{id="position",text=position or clock,width=inner/2,height=18,font_size=CAPTION,color="onSurfaceVariant"},
                ui.Text{id="duration",text=duration or "",width=inner/2,height=18,font_size=CAPTION,text_align="right",color="onSurfaceVariant"}}}}
    end
    local card={id="card",position="absolute",left=0,top=0,width=cardW,height=H,radius=RADIUS,fill="transparent"}
    for _,node in ipairs(backdrop) do card[#card+1]=node end
    for _,node in ipairs(content) do card[#card+1]=node end
    local lyrics
    if hasLyrics then
        local lyricsW=W-cardW-GUTTER
        local lyricsInner=lyricsW-2*PAD
        local lines={}
        if get("dashboard.summary")~="" then
            lines[1]=ui.Text{id="message",text=ui.bind("dashboard.summary"),width=lyricsInner,height=18,font_size=CAPTION,color="onSurfaceVariant"}
        end
        if #rows>0 then
            for i,item in ipairs(rows) do
                lines[#lines+1]=ui.Text{id="line-"..item.id,text=item.label,width=lyricsInner,height=24,font_size=BODY,color=i==1 and "onSurface" or "onSurfaceVariant"}
            end
        else
            lines[#lines+1]=ui.Text{id="plain",text=ui.bind("dashboard.detail"),width=lyricsInner,height=H-2*PAD-36-T.spacing.md,font_size=BODY,color="onSurfaceVariant"}
        end
        lyrics=ui.Column{id="lyrics",position="absolute",left=cardW+GUTTER,top=0,width=lyricsW,height=H,padding=PAD,gap=T.spacing.md,radius=RADIUS,fill="surfaceContainer",
            ui.Text{id="heading",text=ui.bind("dashboard.heading"),width=lyricsInner,height=24,font_size=BODY,font_weight=600,color="onSurface"},
            ui.Column{id="lines",width=lyricsInner,gap=T.spacing.xs,table.unpack(lines)}}
    end
    return ui.Box{id="media",width=W,height=H,ui.Box(card),lyrics}
end
