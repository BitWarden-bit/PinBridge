"""Real-Pin verification of pre-decode policy and decoded-event delivery."""

import pb


TARGET_START = 0
TARGET_END = 0
NAMED_SEEN = False
BATCH_SEEN = False
INSTRUMENTATION_GENERATION = 0


def validate(event):
    if not (TARGET_START <= event["addr"] < TARGET_END):
        raise RuntimeError("native decode range filter leaked address 0x%x" % event["addr"])
    # Named handlers expose descriptive fields. High-volume batch delivery
    # intentionally keeps the compact raw a0..a7 schema.
    size = event.get("size", event["a0"])
    policy_generation = event.get("policy_generation", event["a7"])
    if policy_generation != INSTRUMENTATION_GENERATION:
        raise RuntimeError(
            "decode policy generation %d != %d"
            % (policy_generation, INSTRUMENTATION_GENERATION)
        )
    if "policy_generation" in event:
        if event["next_address"] != event["address"] + size:
            raise RuntimeError("decode next_address is inconsistent")
        for field in (
            "is_direct_control_flow",
            "is_indirect_control_flow",
            "direct_target",
        ):
            if field not in event:
                raise RuntimeError("decode event is missing %s" % field)
    elif "a6" not in event:
        raise RuntimeError("raw decode batch is missing the direct-target slot")
    # Frozen Pin 3.31 XED_CATEGORY_CLDEMOTE value from xed-category-enum.h.
    return event["a1"] == 24


def on_decode(event):
    global NAMED_SEEN
    if validate(event) and not NAMED_SEEN:
        NAMED_SEEN = True
        pb.print(
            "XED_DECODE_NAMED_PASS address=0x%x size=%d category=%d opcode=%d"
            % (event["address"], event["size"], event["category"], event["opcode"])
        )


def on_event_batch(events, missed):
    global BATCH_SEEN
    for event in events:
        if validate(event) and not BATCH_SEEN:
            BATCH_SEEN = True
            pb.print("XED_DECODE_BATCH_PASS missed=%d" % missed)


def pb_init():
    global TARGET_START, TARGET_END, INSTRUMENTATION_GENERATION
    main = next((row[3] for row in pb.modules() if row[2]), None)
    if main is None:
        raise RuntimeError("main module not found")
    main = main.replace("/", "\\").split("\\")[-1]
    TARGET_START = pb.resolve_name(main + "!DecodeTarget")
    if not TARGET_START:
        raise RuntimeError("DecodeTarget export not found")
    TARGET_END = TARGET_START + 32

    generation = pb.xed_decode_set(cldemote=True)
    if pb.xed_decode_policy() != (None, True, None):
        raise RuntimeError("XED decode policy did not round-trip")
    pb.on("instruction.decode", on_decode)
    pb.watch(["instruction.decode"], range=(TARGET_START, TARGET_END), batch=64)
    INSTRUMENTATION_GENERATION = pb.instrumentation_set(
        kinds=["instruction.decode"],
        ranges=[(TARGET_START, TARGET_END)],
    )
    if pb.instrumentation_policy()[0] != ["instruction.decode"]:
        raise RuntimeError("decode instrumentation policy did not round-trip")
    pb.print(
        "XED_POLICY_READY generation=%d instrumentation_generation=%d range=0x%x-0x%x"
        % (generation, INSTRUMENTATION_GENERATION, TARGET_START, TARGET_END)
    )
