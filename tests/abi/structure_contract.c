#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <stdint.h>
#include <stdio.h>

#include "pinbridge/pinbridge.h"

#pragma warning(disable : 4191)

typedef PbStatus(PB_CALL* AddTraceFn)(PbTraceInstrumentCallback, void*, PbCallbackHandle*);
typedef PbStatus(PB_CALL* AddRtnFn)(PbRtnInstrumentCallback, void*, PbCallbackHandle*);
typedef PbStatus(PB_CALL* AddImgFn)(PbImgInstrumentCallback, void*, PbCallbackHandle*);
typedef PbStatus(PB_CALL* BblAddressFn)(PbBblHandle, uint64_t*);
typedef PbStatus(PB_CALL* TraceAddressFn)(PbTraceHandle, uint64_t*);
typedef PbStatus(PB_CALL* RtnAddressFn)(PbRtnHandle, uint64_t*);
typedef PbStatus(PB_CALL* SecAddressFn)(PbSecHandle, uint64_t*);
typedef PbStatus(PB_CALL* ImgEntryAddressFn)(PbImgHandle, uint64_t*);
typedef PbStatus(PB_CALL* RtnQueryFn)(PbRtnHandle, PbRtnHandle*);

static uint32_t g_trace_calls;
static uint32_t g_rtn_calls;
static uint32_t g_img_calls;

static void PB_CALL OnTrace(PbTraceHandle trace, void* user)
{
    if (trace && user == &g_trace_calls)
        ++g_trace_calls;
}

static void PB_CALL OnRtn(PbRtnHandle rtn, void* user)
{
    if (rtn.opaque == 43 && user == &g_rtn_calls)
        ++g_rtn_calls;
}

static void PB_CALL OnImg(PbImgHandle img, void* user)
{
    if (img.opaque == 44 && user == &g_img_calls)
        ++g_img_calls;
}

#define CHECK(condition, message) do { if (!(condition)) { fprintf(stderr, "%s\n", message); return 1; } } while (0)
#define LOAD(type, name) ((type)GetProcAddress(module, name))

int main(int argc, char** argv)
{
    HMODULE module;
    AddTraceFn add_trace;
    AddRtnFn add_rtn;
    AddImgFn add_img;
    BblAddressFn bbl_address;
    TraceAddressFn trace_address;
    RtnAddressFn rtn_address;
    SecAddressFn sec_address;
    ImgEntryAddressFn img_entry_address;
    RtnQueryFn rtn_ifunc_implementation;
    RtnQueryFn rtn_ifunc_resolver;
    PbCallbackHandle callback = {0};
    uint64_t value = 0;
    PbBblHandle bbl = {1};
    PbRtnHandle rtn = {1};
    PbSecHandle sec = {1};
    PbImgHandle img = {1};
    PbTraceHandle trace = (PbTraceHandle)(uintptr_t)0x1000;
    PbRtnHandle rtn_result = {0};

    CHECK(argc == 2, "usage: pb_structure_contract <bridge.dll>");
    module = LoadLibraryA(argv[1]);
    CHECK(module != NULL, "cannot load bridge DLL");
    add_trace = LOAD(AddTraceFn, "pb_trace_add_instrument_function");
    add_rtn = LOAD(AddRtnFn, "pb_rtn_add_instrument_function");
    add_img = LOAD(AddImgFn, "pb_img_add_instrument_function");
    bbl_address = LOAD(BblAddressFn, "pb_bbl_address");
    trace_address = LOAD(TraceAddressFn, "pb_trace_address");
    rtn_address = LOAD(RtnAddressFn, "pb_rtn_address");
    sec_address = LOAD(SecAddressFn, "pb_sec_address");
    img_entry_address = LOAD(ImgEntryAddressFn, "pb_img_entry_address");
    rtn_ifunc_implementation = LOAD(RtnQueryFn, "pb_rtn_i_func_implementation");
    rtn_ifunc_resolver = LOAD(RtnQueryFn, "pb_rtn_i_func_resolver");
    CHECK(add_trace && add_rtn && add_img && bbl_address && trace_address && rtn_address &&
              sec_address && img_entry_address && rtn_ifunc_implementation && rtn_ifunc_resolver,
          "structure symbol missing");
    CHECK(add_trace(NULL, NULL, &callback) == PB_ERR_INVALID_ARGUMENT, "NULL trace callback accepted");
    CHECK(add_trace(OnTrace, &g_trace_calls, &callback) == PB_OK && callback.opaque != 0 &&
              g_trace_calls == 1,
          "trace callback contract failed");
    CHECK(add_rtn(OnRtn, &g_rtn_calls, &callback) == PB_OK && g_rtn_calls == 1,
          "rtn callback contract failed");
    CHECK(add_img(OnImg, &g_img_calls, &callback) == PB_OK && g_img_calls == 1,
          "img callback contract failed");
    CHECK(bbl_address(bbl, &value) == PB_OK, "BBL query failed");
    CHECK(trace_address(trace, &value) == PB_OK, "TRACE query failed");
    CHECK(rtn_address(rtn, &value) == PB_OK, "RTN query failed");
    CHECK(sec_address(sec, &value) == PB_OK, "SEC query failed");
    CHECK(img_entry_address(img, &value) == PB_OK, "IMG query failed");
    CHECK(rtn_ifunc_implementation(rtn, &rtn_result) == PB_ERR_UNSUPPORTED,
          "Windows accepted Linux-only RTN_IFuncImplementation");
    CHECK(rtn_ifunc_resolver(rtn, &rtn_result) == PB_ERR_UNSUPPORTED,
          "Windows accepted Linux-only RTN_IFuncResolver");
    bbl.opaque = 0;
    CHECK(bbl_address(bbl, &value) == PB_ERR_INVALID_ARGUMENT, "invalid BBL accepted");
    CHECK(trace_address(NULL, &value) == PB_ERR_INVALID_ARGUMENT, "invalid TRACE accepted");
    FreeLibrary(module);
    return 0;
}
