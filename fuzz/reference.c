#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

#define QOI_NO_STDIO
#define QOI_IMPLEMENTATION
#include "../reference/qoi/qoi.h"

static int checked_raw_len(unsigned int width, unsigned int height, unsigned int channels, int *out_len) {
	size_t len;

	if (width != 0 && (size_t)height > ((size_t)-1) / (size_t)width) {
		return 0;
	}

	len = (size_t)width * (size_t)height;
	if (channels != 0 && len > ((size_t)-1) / (size_t)channels) {
		return 0;
	}

	len *= (size_t)channels;
	if (len > INT_MAX) {
		return 0;
	}

	*out_len = (int)len;
	return 1;
}

int qoi_ref_encode(
	const unsigned char *pixels,
	unsigned int width,
	unsigned int height,
	unsigned char channels,
	unsigned char colorspace,
	unsigned char **out,
	int *out_len
) {
	qoi_desc desc;
	void *encoded;

	if (pixels == NULL || out == NULL || out_len == NULL) {
		return 0;
	}

	desc.width = width;
	desc.height = height;
	desc.channels = channels;
	desc.colorspace = colorspace;

	encoded = qoi_encode(pixels, &desc, out_len);
	if (encoded == NULL) {
		return 0;
	}

	*out = (unsigned char *)encoded;
	return 1;
}

int qoi_ref_decode(
	const unsigned char *qoi,
	int qoi_len,
	int requested_channels,
	unsigned char **out,
	int *out_len
) {
	qoi_desc desc;
	void *decoded;
	unsigned int output_channels;

	if (qoi == NULL || out == NULL || out_len == NULL || qoi_len < 0) {
		return 0;
	}

	decoded = qoi_decode(qoi, qoi_len, &desc, requested_channels);
	if (decoded == NULL) {
		return 0;
	}

	output_channels = requested_channels == 0 ? desc.channels : (unsigned int)requested_channels;
	if (!checked_raw_len(desc.width, desc.height, output_channels, out_len)) {
		QOI_FREE(decoded);
		return 0;
	}

	*out = (unsigned char *)decoded;
	return 1;
}

void qoi_ref_free(void *ptr) {
	QOI_FREE(ptr);
}
