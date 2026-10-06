pub(crate) const T6_EQUIPMENT: &str = include_str!("t6_equipment.gsc.txt");

pub(crate) const RADIATION: &str = r#"main()
{
    waittillframeend;
    level.radiation_busy = true;
    level.radiation_open = false;
    level.radiation_doors = [];
    level.radiation_doors[0] = getent("big_door1_clip", "targetname");
    level.radiation_doors[1] = getent("big_door2_clip", "targetname");
    switches = [];
    switches[0] = getent("switch_trigger1", "targetname");
    switches[1] = getent("switch_trigger2", "targetname");
    if (!isdefined(level.radiation_doors[0]) || !isdefined(level.radiation_doors[1])
        || !isdefined(switches[0]) || !isdefined(switches[1]))
        return;
    models = [];
    models[0] = getent("big_door1", "targetname");
    models[1] = getent("big_door2", "targetname");
    for (i = 0; i < 2; i++)
    {
        if (isdefined(models[i]))
            models[i] linkto(level.radiation_doors[i]);
        switches[i] usetriggerrequirelookat();
        switches[i] setcursorhint("HINT_ACTIVATE");
        switches[i] sethintstring(&"MP_HOLD_TO_OPERATE_DOORS");
        switches[i] thread watch_switch();
    }
    level.radiation_switches = switches;
    if (!isdefined(level.gameended) || !level.gameended)
    {
        level waittill("prematch_over");
        level.radiation_busy = false;
        thread operate_doors();
    }
}

watch_switch()
{
    for (;;)
    {
        self waittill("trigger", player);
        if (!level.radiation_busy)
        {
            if (isdefined(player))
                player playsound("t5:evt_hydraulic_switch");
            thread operate_doors();
        }
    }
}

operate_doors()
{
    if (level.radiation_busy)
        return;
    level.radiation_busy = true;
    for (i = 0; i < 2; i++)
        level.radiation_switches[i] makeunusable();
    turn = 123;
    if (level.radiation_open)
        turn = -turn;
    duration = getdvarfloat("scr_d1_time");
    if (duration <= 0)
        duration = 8;
    cooldown = getdvarfloat("scr_door_cooldown");
    if (cooldown <= 0)
        cooldown = 20;
    level.radiation_doors[0] rotateroll(turn, duration, duration * .6, duration * .4);
    level.radiation_doors[1] rotateroll(-turn, duration, duration * .7, duration * .3);
    level.radiation_doors[0] playloopsound("evt_hydraulic_loop");
    level.radiation_doors[0] playsound("t5:evt_hydraulic_start");
    level.radiation_doors[0] waittill("rotatedone");
    level.radiation_doors[0] stoploopsound();
    level.radiation_open = !level.radiation_open;
    if (level.radiation_open)
        level.radiation_doors[0] playsound("t5:evt_hydraulic_open");
    else
        level.radiation_doors[0] playsound("t5:evt_hydraulic_close");
    wait cooldown;
    level.radiation_busy = false;
    for (i = 0; i < 2; i++)
        level.radiation_switches[i] makeusable();
}
"#;

pub(crate) const DESTRUCTIBLES: &str = r#"main()
{
    level.globalCarsDestroyed = 0;
}

broken(event, attacker)
{
    if (event == "destructible_car_fire" || event == "destructible_barrel_fire")
        self thread ignite(attacker);
    else if (event == "destructible_car_explosion" || event == "destructible_barrel_explosion" || event == "explode")
        self explode(attacker);
}

ignite(attacker)
{
    self endon("wrecked");
    wait randomintrange(7, 10);
    self thread explode(attacker);
}

explode(attacker)
{
    if (isdefined(self.wrecked) && self.wrecked)
        return;
    self.wrecked = true;
    self notify("wrecked");
    self damagepiece(20000, -1, attacker);
    self radiusdamage(self.origin, 256, 300, 75, attacker, "MOD_EXPLOSIVE");
    earthquake(.5, .5, self.origin, 800);
    level.globalCarsDestroyed++;
}

break_after(piece, stage, duration, amount, attacker)
{
    wait duration;
    if (self piecestage(piece, stage))
        self damagepiece(amount, piece, attacker);
}
"#;
