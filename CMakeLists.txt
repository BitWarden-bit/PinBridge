cmake_minimum_required(VERSION 3.24)
project(pinbridge_contract_tests VERSION 1.0.0 LANGUAGES C CXX)

enable_testing()

add_library(pinbridge_headers INTERFACE)
target_include_directories(pinbridge_headers INTERFACE "${CMAKE_CURRENT_SOURCE_DIR}/include")
target_compile_definitions(pinbridge_headers INTERFACE PB_STATIC)

add_executable(pb_header_c_test tests/abi/header_c_test.c)
target_link_libraries(pb_header_c_test PRIVATE pinbridge_headers)
set_property(TARGET pb_header_c_test PROPERTY C_STANDARD 11)
add_test(NAME abi.header.c COMMAND pb_header_c_test)

add_library(pinbridge_mock SHARED
    src/pinbridge_api.cpp
    src/pinbridge_context.cpp
    src/pinbridge_context_constants.cpp
    src/pinbridge_control_queries.cpp
    src/pinbridge_control_attach.cpp
    src/pinbridge_control_callbacks.cpp
    src/pinbridge_control_fetch.cpp
    src/pinbridge_control_internal_exception.cpp
    src/pinbridge_control_memory_translation.cpp
    src/pinbridge_control_out_of_memory.cpp
    src/pinbridge_control_probe_detach.cpp
    src/pinbridge_process_follow_child.cpp
    src/pinbridge_control_insert_call_probed.cpp
    src/pinbridge_control_call_application.cpp
    src/pinbridge_bbl_instrumentation.cpp
    src/pinbridge_trace_instrumentation.cpp
    src/pinbridge_trace_smc.cpp
    src/pinbridge_trace_version.cpp
    src/pinbridge_error_file.cpp
    src/pinbridge_callbacks.cpp
    src/pinbridge_physical_context.cpp
    src/pinbridge_process.cpp
    src/pinbridge_deprecated.cpp
    src/pinbridge_replay.cpp
    src/pinbridge_ins_modification.cpp
    src/pinbridge_buffer.cpp
    src/pinbridge_img.cpp
    src/pinbridge_syscall.cpp
    src/pinbridge_symbols.cpp
    src/pinbridge_proto.cpp
    src/pinbridge_inst_args.cpp
    src/pinbridge_rtn_varargs.cpp
    src/pinbridge_knobs.cpp
    src/pinbridge_appdebug.cpp
    src/pinbridge_exception.cpp
    src/pinbridge_lock.cpp
    src/pinbridge_message.cpp
    src/pinbridge_threads.cpp
    src/pinbridge_ins_instrumentation.cpp
    src/pinbridge_ins_inspection_extras.cpp
    src/pinbridge_ins_capture.cpp
    src/pinbridge_disasm.cpp
    src/pinbridge_utils_core.cpp
    src/pinbridge_utils_format.cpp
    src/pinbridge_control_state.cpp
    src/pinbridge_debug_info.cpp
    src/pinbridge_ins_queries.cpp
    src/pinbridge_reg_queries.cpp
    src/pinbridge_reg_functions.cpp
    src/pinbridge_regset.cpp
    src/pinbridge_sec.cpp
    src/pinbridge_structure_queries.cpp
    src/pinbridge_structure_callbacks.cpp
    tests/mock/mock_backend.cpp
    tests/mock/mock_context_backend.cpp
    tests/mock/mock_context_constant_backend.cpp
    tests/mock/mock_control_query_backend.cpp
    tests/mock/mock_control_attach_backend.cpp
    tests/mock/mock_control_callback_backend.cpp
    tests/mock/mock_control_fetch_backend.cpp
    tests/mock/mock_control_internal_exception_backend.cpp
    tests/mock/mock_control_memory_translation_backend.cpp
    tests/mock/mock_control_out_of_memory_backend.cpp
    tests/mock/mock_control_probe_detach_backend.cpp
    tests/mock/mock_process_follow_child_backend.cpp
    tests/mock/mock_control_insert_call_probed_backend.cpp
    tests/mock/mock_control_call_application_backend.cpp
    tests/mock/mock_bbl_instrumentation_backend.cpp
    tests/mock/mock_trace_instrumentation_backend.cpp
    tests/mock/mock_trace_smc_backend.cpp
    tests/mock/mock_trace_version_backend.cpp
    tests/mock/mock_error_file_backend.cpp
    tests/mock/mock_callbacks_backend.cpp
    tests/mock/mock_physical_context_backend.cpp
    tests/mock/mock_process_backend.cpp
    tests/mock/mock_deprecated_backend.cpp
    tests/mock/mock_replay_backend.cpp
    tests/mock/mock_ins_modification_backend.cpp
    tests/mock/mock_buffer_backend.cpp
    tests/mock/mock_img_backend.cpp
    tests/mock/mock_syscall_backend.cpp
    tests/mock/mock_symbols_backend.cpp
    tests/mock/mock_proto_backend.cpp
    tests/mock/mock_inst_args_backend.cpp
    tests/mock/mock_rtn_varargs_backend.cpp
    tests/mock/mock_knobs_backend.cpp
    tests/mock/mock_appdebug_backend.cpp
    tests/mock/mock_exception_backend.cpp
    tests/mock/mock_lock_backend.cpp
    tests/mock/mock_message_backend.cpp
    tests/mock/mock_threads_backend.cpp
    tests/mock/mock_ins_instrumentation_backend.cpp
    tests/mock/mock_ins_inspection_extras_backend.cpp
    tests/mock/mock_ins_capture_backend.cpp
    tests/mock/mock_disasm_backend.cpp
    tests/mock/mock_utils_core_backend.cpp
    tests/mock/mock_utils_format_backend.cpp
    tests/mock/mock_control_state_backend.cpp
    tests/mock/mock_debug_info_backend.cpp
    tests/mock/mock_query_backend.cpp
    tests/mock/mock_reg_query_backend.cpp
    tests/mock/mock_reg_function_backend.cpp
    tests/mock/mock_regset_backend.cpp
    tests/mock/mock_sec_backend.cpp
    tests/mock/mock_structure_query_backend.cpp
    tests/mock/mock_structure_callback_backend.cpp
)
target_include_directories(pinbridge_mock PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/include"
    "${CMAKE_CURRENT_SOURCE_DIR}/src"
)
target_compile_definitions(pinbridge_mock PRIVATE PB_BUILDING_DLL)
set_property(TARGET pinbridge_mock PROPERTY CXX_STANDARD 17)
set_property(TARGET pinbridge_mock PROPERTY CXX_STANDARD_REQUIRED ON)

add_executable(pb_runtime_fixture tests/runtime/fixture.c)
set_property(TARGET pb_runtime_fixture PROPERTY C_STANDARD 11)

if(WIN32)
    add_executable(pb_rpc_fixture tests/runtime/rpc_fixture.c)
    set_property(TARGET pb_rpc_fixture PROPERTY C_STANDARD 11)

    add_executable(pb_single_step_fixture tests/runtime/single_step_fixture.c)
    set_property(TARGET pb_single_step_fixture PROPERTY C_STANDARD 11)
    add_test(NAME runtime.single_step.native COMMAND pb_single_step_fixture)
endif()

if(WIN32)
    add_executable(pb_contract_loader tests/abi/contract_loader.c)
    target_link_libraries(pb_contract_loader PRIVATE pinbridge_headers)
    set_property(TARGET pb_contract_loader PROPERTY C_STANDARD 11)
    add_test(NAME abi.dynamic.contract COMMAND pb_contract_loader "$<TARGET_FILE:pinbridge_mock>")

    add_executable(pb_generated_exports tests/abi/generated_exports.c)
    target_link_libraries(pb_generated_exports PRIVATE pinbridge_headers)
    set_property(TARGET pb_generated_exports PROPERTY C_STANDARD 11)
    add_test(NAME abi.generated.exports COMMAND pb_generated_exports "$<TARGET_FILE:pinbridge_mock>")

    add_executable(pb_structure_contract tests/abi/structure_contract.c)
    target_link_libraries(pb_structure_contract PRIVATE pinbridge_headers)
    set_property(TARGET pb_structure_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.structure.contract COMMAND pb_structure_contract "$<TARGET_FILE:pinbridge_mock>")

    add_executable(pb_regset_contract tests/abi/regset_contract.c)
    target_link_libraries(pb_regset_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_regset_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.regset.contract COMMAND pb_regset_contract)

    add_executable(pb_context_contract tests/abi/context_contract.c)
    target_link_libraries(pb_context_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_context_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.context.contract COMMAND pb_context_contract)

    add_executable(pb_reg_functions_contract tests/abi/reg_functions_contract.c)
    target_link_libraries(pb_reg_functions_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_reg_functions_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.reg.functions.contract COMMAND pb_reg_functions_contract)

    add_executable(pb_control_queries_contract tests/abi/control_queries_contract.c)
    target_link_libraries(pb_control_queries_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_queries_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.queries.contract COMMAND pb_control_queries_contract)

    add_executable(pb_debug_info_contract tests/abi/debug_info_contract.c)
    target_link_libraries(pb_debug_info_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_debug_info_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.debug.info.contract COMMAND pb_debug_info_contract)

    add_executable(pb_sec_contract tests/abi/sec_contract.c)
    target_link_libraries(pb_sec_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_sec_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.sec.contract COMMAND pb_sec_contract)

    add_executable(pb_control_callbacks_contract tests/abi/control_callbacks_contract.c)
    target_link_libraries(pb_control_callbacks_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_callbacks_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.callbacks.contract COMMAND pb_control_callbacks_contract)

    add_executable(pb_thread_callbacks_contract tests/abi/thread_callbacks_contract.c)
    target_link_libraries(pb_thread_callbacks_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_thread_callbacks_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.thread.callbacks.contract COMMAND pb_thread_callbacks_contract)

    add_executable(pb_control_state_contract tests/abi/control_state_contract.c)
    target_link_libraries(pb_control_state_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_state_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.state.contract COMMAND pb_control_state_contract)

    add_executable(pb_control_fetch_contract tests/abi/control_fetch_contract.c)
    target_link_libraries(pb_control_fetch_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_fetch_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.fetch.contract COMMAND pb_control_fetch_contract)

    add_executable(pb_control_internal_exception_contract
        tests/abi/control_internal_exception_contract.c)
    target_link_libraries(pb_control_internal_exception_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_internal_exception_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.internal_exception.contract
        COMMAND pb_control_internal_exception_contract)

    add_executable(pb_control_memory_translation_contract
        tests/abi/control_memory_translation_contract.c)
    target_link_libraries(pb_control_memory_translation_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_memory_translation_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.memory_translation.contract
        COMMAND pb_control_memory_translation_contract)

    add_executable(pb_control_safe_copy_ex_contract
        tests/abi/control_safe_copy_ex_contract.c)
    target_link_libraries(pb_control_safe_copy_ex_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_safe_copy_ex_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.safe_copy_ex.contract
        COMMAND pb_control_safe_copy_ex_contract)

    add_executable(pb_control_probe_detach_contract
        tests/abi/control_probe_detach_contract.c)
    target_link_libraries(pb_control_probe_detach_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_probe_detach_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.probe_detach.contract
        COMMAND pb_control_probe_detach_contract)

    add_executable(pb_control_jit_detach_contract
        tests/abi/control_jit_detach_contract.c)
    target_link_libraries(pb_control_jit_detach_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_jit_detach_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.jit_detach.contract
        COMMAND pb_control_jit_detach_contract)

    add_executable(pb_control_out_of_memory_contract
        tests/abi/control_out_of_memory_contract.c)
    target_link_libraries(pb_control_out_of_memory_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_out_of_memory_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.out_of_memory.contract
        COMMAND pb_control_out_of_memory_contract)

    add_executable(pb_control_attach_contract
        tests/abi/control_attach_contract.c)
    target_link_libraries(pb_control_attach_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_attach_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.attach.contract
        COMMAND pb_control_attach_contract)

    add_executable(pb_process_follow_child_contract
        tests/abi/process_follow_child_contract.c)
    target_link_libraries(pb_process_follow_child_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_process_follow_child_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.process.follow_child.contract
        COMMAND pb_process_follow_child_contract)

    add_executable(pb_control_insert_call_probed_contract
        tests/abi/control_insert_call_probed_contract.c)
    target_link_libraries(pb_control_insert_call_probed_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_insert_call_probed_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.insert_call_probed.contract
        COMMAND pb_control_insert_call_probed_contract)

    add_executable(pb_control_call_application_contract
        tests/abi/control_call_application_contract.c)
    target_link_libraries(pb_control_call_application_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_control_call_application_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.control.call_application.contract
        COMMAND pb_control_call_application_contract)

    add_executable(pb_bbl_instrumentation_contract
        tests/abi/bbl_instrumentation_contract.c)
    target_link_libraries(pb_bbl_instrumentation_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_bbl_instrumentation_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.bbl.instrumentation.contract
        COMMAND pb_bbl_instrumentation_contract)

    add_executable(pb_trace_instrumentation_contract
        tests/abi/trace_instrumentation_contract.c)
    target_link_libraries(pb_trace_instrumentation_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_trace_instrumentation_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.trace.instrumentation.contract
        COMMAND pb_trace_instrumentation_contract)

    add_executable(pb_trace_smc_contract tests/abi/trace_smc_contract.c)
    target_link_libraries(pb_trace_smc_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_trace_smc_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.trace.smc.contract COMMAND pb_trace_smc_contract)

    add_executable(pb_trace_version_contract tests/abi/trace_version_contract.c)
    target_link_libraries(pb_trace_version_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_trace_version_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.trace.version.contract COMMAND pb_trace_version_contract)

    add_executable(pb_error_file_contract tests/abi/error_file_contract.c)
    target_link_libraries(pb_error_file_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_error_file_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.error.file.contract COMMAND pb_error_file_contract)

    add_executable(pb_callbacks_contract tests/abi/callbacks_contract.c)
    target_link_libraries(pb_callbacks_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_callbacks_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.callbacks.contract COMMAND pb_callbacks_contract)

    add_executable(pb_physical_context_contract tests/abi/physical_context_contract.c)
    target_link_libraries(pb_physical_context_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_physical_context_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.physical.context.contract COMMAND pb_physical_context_contract)

    add_executable(pb_process_contract tests/abi/process_contract.c)
    target_link_libraries(pb_process_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_process_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.process.contract COMMAND pb_process_contract)

    add_executable(pb_deprecated_contract tests/abi/deprecated_contract.c)
    target_link_libraries(pb_deprecated_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_deprecated_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.deprecated.contract COMMAND pb_deprecated_contract)

    add_executable(pb_replay_contract tests/abi/replay_contract.c)
    target_link_libraries(pb_replay_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_replay_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.replay.contract COMMAND pb_replay_contract)

    add_executable(pb_ins_modification_contract
        tests/abi/ins_modification_contract.c)
    target_link_libraries(pb_ins_modification_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_ins_modification_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.ins.modification.contract COMMAND pb_ins_modification_contract)

    add_executable(pb_buffer_contract tests/abi/buffer_contract.c)
    target_link_libraries(pb_buffer_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_buffer_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.buffer.contract COMMAND pb_buffer_contract)

    add_executable(pb_img_contract tests/abi/img_contract.c)
    target_link_libraries(pb_img_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_img_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.img.contract COMMAND pb_img_contract)

    add_executable(pb_syscall_contract tests/abi/syscall_contract.c)
    target_link_libraries(pb_syscall_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_syscall_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.syscall.contract COMMAND pb_syscall_contract)

    add_executable(pb_symbols_contract tests/abi/symbols_contract.c)
    target_link_libraries(pb_symbols_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_symbols_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.symbols.contract COMMAND pb_symbols_contract)

    add_executable(pb_proto_contract tests/abi/proto_contract.c)
    target_link_libraries(pb_proto_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_proto_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.proto.contract COMMAND pb_proto_contract)

    add_executable(pb_ins_instrumentation_contract
        tests/abi/ins_instrumentation_contract.c)
    target_link_libraries(pb_ins_instrumentation_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_ins_instrumentation_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.ins.instrumentation.contract
        COMMAND pb_ins_instrumentation_contract)

    add_executable(pb_ins_inspection_extras_contract
        tests/abi/ins_inspection_extras_contract.c)
    target_link_libraries(pb_ins_inspection_extras_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_ins_inspection_extras_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.ins.inspection.extras.contract
        COMMAND pb_ins_inspection_extras_contract)

    add_executable(pb_ins_capture_contract
        tests/abi/ins_capture_contract.c)
    target_link_libraries(pb_ins_capture_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_ins_capture_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.ins.capture.contract
        COMMAND pb_ins_capture_contract)

    add_executable(pb_disasm_contract
        tests/abi/disasm_contract.c)
    target_link_libraries(pb_disasm_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_disasm_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.disasm.contract
        COMMAND pb_disasm_contract)

    add_executable(pb_utils_core_contract tests/abi/utils_core_contract.c)
    target_link_libraries(pb_utils_core_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_utils_core_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.utils.core.contract COMMAND pb_utils_core_contract)

    add_executable(pb_utils_mem_range_contract
        tests/abi/utils_mem_range_contract.c)
    target_link_libraries(pb_utils_mem_range_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_utils_mem_range_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.utils.mem_range.contract
        COMMAND pb_utils_mem_range_contract)

    add_executable(pb_utils_format_contract tests/abi/utils_format_contract.c)
    target_link_libraries(pb_utils_format_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_utils_format_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.utils.format.contract COMMAND pb_utils_format_contract)

    add_executable(pb_utils_legacy_format_contract
        tests/abi/utils_legacy_format_contract.c)
    target_link_libraries(pb_utils_legacy_format_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_utils_legacy_format_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.utils.legacy_format.contract
        COMMAND pb_utils_legacy_format_contract)

    add_executable(pb_rtn_lookup_contract tests/abi/rtn_lookup_contract.c)
    target_link_libraries(pb_rtn_lookup_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_rtn_lookup_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.rtn.lookup.contract COMMAND pb_rtn_lookup_contract)

    add_executable(pb_rtn_lifecycle_contract
        tests/abi/rtn_lifecycle_contract.c)
    target_link_libraries(pb_rtn_lifecycle_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_rtn_lifecycle_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.rtn.lifecycle.contract COMMAND pb_rtn_lifecycle_contract)

    add_executable(pb_rtn_replacement_contract
        tests/abi/rtn_replacement_contract.c)
    target_link_libraries(pb_rtn_replacement_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_rtn_replacement_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.rtn.replacement.contract COMMAND pb_rtn_replacement_contract)

    add_executable(pb_inst_args_contract tests/abi/inst_args_contract.c)
    target_link_libraries(pb_inst_args_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_inst_args_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.inst.args.contract COMMAND pb_inst_args_contract)

    add_executable(pb_rtn_varargs_contract tests/abi/rtn_varargs_contract.c)
    target_link_libraries(pb_rtn_varargs_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_rtn_varargs_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.rtn.varargs.contract COMMAND pb_rtn_varargs_contract)

    add_executable(pb_utils_completion_contract
        tests/abi/utils_completion_contract.c)
    target_link_libraries(pb_utils_completion_contract
        PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_utils_completion_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.utils.completion.contract
        COMMAND pb_utils_completion_contract)
    add_executable(pb_knobs_contract tests/abi/knobs_contract.c)
    target_link_libraries(pb_knobs_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_knobs_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.knobs.contract COMMAND pb_knobs_contract)
    add_executable(pb_appdebug_contract tests/abi/appdebug_contract.c)
    target_link_libraries(pb_appdebug_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_appdebug_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.appdebug.contract COMMAND pb_appdebug_contract)
    add_executable(pb_exception_contract tests/abi/exception_contract.c)
    target_link_libraries(pb_exception_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_exception_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.exception.contract COMMAND pb_exception_contract)

    add_executable(pb_lock_contract tests/abi/lock_contract.c)
    target_link_libraries(pb_lock_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_lock_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.lock.contract COMMAND pb_lock_contract)

    add_executable(pb_message_contract tests/abi/message_contract.c)
    target_link_libraries(pb_message_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_message_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.message.contract COMMAND pb_message_contract)

    add_executable(pb_threads_contract tests/abi/threads_contract.c)
    target_link_libraries(pb_threads_contract PRIVATE pinbridge_headers pinbridge_mock)
    set_property(TARGET pb_threads_contract PROPERTY C_STANDARD 11)
    add_test(NAME abi.threads.contract COMMAND pb_threads_contract)
endif()

if(MSVC)
    target_compile_options(pinbridge_mock PRIVATE /W4 /WX)
    target_compile_options(pb_header_c_test PRIVATE /W4 /WX)
    target_compile_options(pb_runtime_fixture PRIVATE /W4 /WX)
    target_compile_options(pb_runtime_fixture PRIVATE /arch:AVX2)
    if(TARGET pb_contract_loader)
        target_compile_options(pb_contract_loader PRIVATE /W4 /WX)
        target_compile_options(pb_generated_exports PRIVATE /W4 /WX)
        target_compile_options(pb_structure_contract PRIVATE /W4 /WX)
        target_compile_options(pb_regset_contract PRIVATE /W4 /WX)
        target_compile_options(pb_context_contract PRIVATE /W4 /WX)
        target_compile_options(pb_reg_functions_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_queries_contract PRIVATE /W4 /WX)
        target_compile_options(pb_debug_info_contract PRIVATE /W4 /WX)
        target_compile_options(pb_sec_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_callbacks_contract PRIVATE /W4 /WX)
        target_compile_options(pb_thread_callbacks_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_state_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_fetch_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_internal_exception_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_memory_translation_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_safe_copy_ex_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_probe_detach_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_jit_detach_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_out_of_memory_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_attach_contract PRIVATE /W4 /WX)
        target_compile_options(pb_process_follow_child_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_insert_call_probed_contract PRIVATE /W4 /WX)
        target_compile_options(pb_control_call_application_contract PRIVATE /W4 /WX)
        target_compile_options(pb_bbl_instrumentation_contract PRIVATE /W4 /WX)
        target_compile_options(pb_trace_instrumentation_contract PRIVATE /W4 /WX)
        target_compile_options(pb_trace_smc_contract PRIVATE /W4 /WX)
        target_compile_options(pb_trace_version_contract PRIVATE /W4 /WX)
        target_compile_options(pb_error_file_contract PRIVATE /W4 /WX)
        target_compile_options(pb_callbacks_contract PRIVATE /W4 /WX)
        target_compile_options(pb_physical_context_contract PRIVATE /W4 /WX)
        target_compile_options(pb_process_contract PRIVATE /W4 /WX)
        target_compile_options(pb_deprecated_contract PRIVATE /W4 /WX)
        target_compile_options(pb_replay_contract PRIVATE /W4 /WX)
        target_compile_options(pb_ins_modification_contract PRIVATE /W4 /WX)
        target_compile_options(pb_buffer_contract PRIVATE /W4 /WX)
        target_compile_options(pb_img_contract PRIVATE /W4 /WX)
        target_compile_options(pb_syscall_contract PRIVATE /W4 /WX)
        target_compile_options(pb_symbols_contract PRIVATE /W4 /WX)
        target_compile_options(pb_proto_contract PRIVATE /W4 /WX)
        target_compile_options(pb_ins_instrumentation_contract PRIVATE /W4 /WX)
        target_compile_options(pb_ins_inspection_extras_contract PRIVATE /W4 /WX)
        target_compile_options(pb_ins_capture_contract PRIVATE /W4 /WX)
        target_compile_options(pb_disasm_contract PRIVATE /W4 /WX)
        target_compile_options(pb_utils_core_contract PRIVATE /W4 /WX)
        target_compile_options(pb_utils_mem_range_contract PRIVATE /W4 /WX)
        target_compile_options(pb_utils_format_contract PRIVATE /W4 /WX)
        target_compile_options(pb_utils_legacy_format_contract PRIVATE /W4 /WX)
        target_compile_options(pb_rtn_lookup_contract PRIVATE /W4 /WX)
        target_compile_options(pb_rtn_lifecycle_contract PRIVATE /W4 /WX)
        target_compile_options(pb_rtn_replacement_contract PRIVATE /W4 /WX)
        target_compile_options(pb_inst_args_contract PRIVATE /W4 /WX)
        target_compile_options(pb_rtn_varargs_contract PRIVATE /W4 /WX)
        target_compile_options(pb_utils_completion_contract PRIVATE /W4 /WX)
        target_compile_options(pb_knobs_contract PRIVATE /W4 /WX)
        target_compile_options(pb_appdebug_contract PRIVATE /W4 /WX)
        target_compile_options(pb_exception_contract PRIVATE /W4 /WX)
        target_compile_options(pb_lock_contract PRIVATE /W4 /WX)
        target_compile_options(pb_message_contract PRIVATE /W4 /WX)
        target_compile_options(pb_threads_contract PRIVATE /W4 /WX)
    endif()
endif()
