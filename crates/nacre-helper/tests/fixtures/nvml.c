/* Developer-only synthetic NVML provider; never queries or changes hardware. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
typedef struct { unsigned int gpu, memory; } utilization_t;
int nvmlInit_v2(void) { return getenv("NACRE_TEST_NVML_FAIL") ? 9 : 0; }
int nvmlShutdown(void) { return 0; }
int nvmlSystemGetDriverVersion(char *out, unsigned int size) { snprintf(out, size, "580.0"); return 0; }
int nvmlDeviceGetHandleByPciBusId_v2(const char *id, void **out) {
    if (getenv("NACRE_TEST_NVML_STALL")) sleep(4);
    if (strcmp(id, "0000:01:00.0") == 0) *out = (void *)(uintptr_t)1;
    else if (strcmp(id, "0000:02:00.0") == 0) *out = (void *)(uintptr_t)2;
    else return 6;
    return 0;
}
int nvmlDeviceGetName(void *device, char *out, unsigned int size) { snprintf(out, size, "%s", (uintptr_t)device == 1 ? "NVIDIA Fixture" : "NVIDIA Other"); return 0; }
int nvmlDeviceGetTemperature(void *device, unsigned int sensor, unsigned int *out) { (void)sensor; if ((uintptr_t)device == 2) return 3; *out = 60; return 0; }
int nvmlDeviceGetClockInfo(void *device, unsigned int clock, unsigned int *out) { (void)clock; if ((uintptr_t)device == 2) return 3; *out = 210; return 0; }
int nvmlDeviceGetUtilizationRates(void *device, utilization_t *out) { out->gpu = (uintptr_t)device == 1 ? 0 : 50; out->memory = 0; return 0; }
