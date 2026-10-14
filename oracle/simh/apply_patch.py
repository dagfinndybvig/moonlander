#!/usr/bin/env python3
"""Apply the oracle's determinism patch to an open-simh checkout.

Usage: python3 -I apply_patch.py /path/to/simh

Adds two registers, both default 0 (= stock behaviour):

  CLK IPT  fixed number of CPU instructions between KW11-L line clock ticks.
           When nonzero the clock is no longer calibrated against wall time
           and is not re-armed by RESET (the KW11-L is a free-running line
           frequency source; RESET only clears its CSR).
  VT  IPC  fixed number of CPU instructions between VT11 display processor
           cycles (one display file word per cycle). When nonzero the VT11 is
           scheduled in instructions instead of wall-clock microseconds.

The edits are exact string replacements; the script refuses to run if any
anchor is missing or appears more than once. CRLF line endings are kept.
"""
import sys
import os

EDITS = {
    "PDP11/pdp11_stddev.c": [
        ("int32 clk_fnxm = 0;                                     /* force NXM on reg */\n",
         "int32 clk_fnxm = 0;                                     /* force NXM on reg */\n"
         "int32 clk_ipt = 0;                                      /* ORACLE: fixed instructions per tick, 0 = calibrated */\n"),
        ("    { FLDATA (FNXM, clk_fnxm, 0), REG_HIDDEN },\n",
         "    { FLDATA (FNXM, clk_fnxm, 0), REG_HIDDEN },\n"
         "    { DRDATAD (IPT, clk_ipt, 32, \"ORACLE: fixed instructions per tick, 0 = calibrated wall clock\"), PV_LEFT },\n"),
        ("t = sim_rtcn_calb (clk_tps, TMR_CLK);                   /* calibrate clock */\n",
         "if (clk_ipt) {                                          /* ORACLE: deterministic tick */\n"
         "    sim_activate (uptr, clk_ipt);\n"
         "    tmr_poll = clk_ipt;\n"
         "    tmxr_poll = clk_ipt;\n"
         "    return SCPE_OK;\n"
         "    }\n"
         "t = sim_rtcn_calb (clk_tps, TMR_CLK);                   /* calibrate clock */\n"),
        ("tmr_poll = sim_rtcn_init_unit (&clk_unit, clk_unit.wait, TMR_CLK);/* init line clock */\n",
         "if (clk_ipt) {                                          /* ORACLE: deterministic tick */\n"
         "    sim_rtcn_calb (0, TMR_CLK);                         /* stop calibration of timer 0 */\n"
         "    sim_register_clock_unit_tmr (NULL, TMR_CLK);        /* detach unit from timer services */\n"
         "    if (!sim_is_active (&clk_unit))                     /* line clock free-runs across RESET */\n"
         "        sim_activate (&clk_unit, clk_ipt);\n"
         "    tmr_poll = clk_ipt;\n"
         "    tmxr_poll = clk_ipt;\n"
         "    return SCPE_OK;\n"
         "    }\n"
         "tmr_poll = sim_rtcn_init_unit (&clk_unit, clk_unit.wait, TMR_CLK);/* init line clock */\n"),
    ],
    "PDP11/pdp11_vt.c": [
        ("    UDATA (&vt_svc, 0, 0),      VT11_DELAY};\n",
         "    UDATA (&vt_svc, 0, 0),      VT11_DELAY};\n"
         "\n"
         "int32 vt_ipc = 0;           /* ORACLE: CPU instructions per VT11 cycle, 0 = wall clock */\n"),
        ("    { DRDATAD (CYCLE,  vt_unit.wait, 24, \"VT11/VS60 cycle\"), REG_NZ + PV_LEFT },\n",
         "    { DRDATAD (CYCLE,  vt_unit.wait, 24, \"VT11/VS60 cycle\"), REG_NZ + PV_LEFT },\n"
         "    { DRDATAD (IPC,    vt_ipc, 32, \"ORACLE: fixed CPU instructions per VT11 cycle, 0 = wall clock\"), PV_LEFT },\n"),
        ("    if (vt11_cycle(uptr->wait, 0))\n"
         "        sim_activate_after (uptr, uptr->wait);  /* running; reschedule */\n",
         "    if (vt11_cycle(uptr->wait, 0)) {\n"
         "        if (vt_ipc)                             /* ORACLE: deterministic */\n"
         "            sim_activate (uptr, vt_ipc);\n"
         "        else\n"
         "            sim_activate_after (uptr, uptr->wait);  /* running; reschedule */\n"
         "        }\n"),
    ],
}


def main():
    root = sys.argv[1]
    for rel, edits in EDITS.items():
        path = os.path.join(root, rel)
        raw = open(path, "rb").read()
        crlf = b"\r\n" in raw
        text = raw.decode("latin-1")
        if crlf:
            text = text.replace("\r\n", "\n")
        for old, new in edits:
            n = text.count(old)
            if n != 1:
                sys.exit("%s: anchor found %d times: %r" % (rel, n, old[:60]))
            text = text.replace(old, new)
        if crlf:
            text = text.replace("\n", "\r\n")
        open(path, "wb").write(text.encode("latin-1"))
        print("patched", rel)


if __name__ == "__main__":
    main()
