// Must compile with the runner's already-installed x64 Windows SDK before query.
// Compile-only: no process/events/registry/privilege/tracing APIs are called.
#include <stddef.h>
#include <windows.h>
#include <evntprov.h>
#include <tdh.h>
_Static_assert(sizeof(void*) == 8, "x64 required");
_Static_assert(sizeof(GUID) == 16, "GUID layout");
_Static_assert(sizeof(EVENT_DESCRIPTOR) == 16, "event descriptor layout");
_Static_assert(offsetof(TRACE_EVENT_INFO, PropertyCount) == 100, "property count offset");
_Static_assert(offsetof(TRACE_EVENT_INFO, TopLevelPropertyCount) == 104, "top-level count offset");
_Static_assert(offsetof(TRACE_EVENT_INFO, EventPropertyInfoArray) == 112, "property array offset");
_Static_assert(sizeof(EVENT_PROPERTY_INFO) == 24, "property element size");
_Static_assert(sizeof(PAYLOAD_FILTER_PREDICATE) == 24, "predicate layout");
int capability_layout_compile_only(void) { return 0; }

typedef TDHSTATUS (WINAPI *DeleteFilterSignature)(PVOID *);
DeleteFilterSignature capability_delete_filter_signature(void) { return &TdhDeletePayloadFilter; }

_Static_assert(sizeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION) == 144, "job extended layout");
_Static_assert(offsetof(JOBOBJECT_BASIC_LIMIT_INFORMATION, LimitFlags) == 16, "job flags offset");
_Static_assert(sizeof(JOBOBJECT_BASIC_ACCOUNTING_INFORMATION) == 48, "job accounting layout");
_Static_assert(offsetof(JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, ActiveProcesses) == 40, "active processes offset");
