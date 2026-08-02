#include <errno.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define QOI_NO_STDIO
#define QOI_IMPLEMENTATION
#include "../../reference/qoi/qoi.h"

static void usage(void) {
	fprintf(stderr, "usage:\n");
	fprintf(stderr, "  qoi-ref encode <width> <height> <channels> <colorspace> <raw-input> <qoi-output>\n");
	fprintf(stderr, "  qoi-ref decode <requested-channels> <qoi-input> <raw-output>\n");
}

static int parse_uint(const char *text, unsigned int *out) {
	char *end = NULL;
	unsigned long value;

	errno = 0;
	value = strtoul(text, &end, 10);
	if (errno != 0 || end == text || *end != '\0' || value > UINT_MAX) {
		return 0;
	}

	*out = (unsigned int)value;
	return 1;
}

static int checked_raw_len(unsigned int width, unsigned int height, unsigned int channels, size_t *out) {
	const size_t max = (size_t)-1;
	size_t len;

	if (width != 0 && (size_t)height > max / (size_t)width) {
		return 0;
	}

	len = (size_t)width * (size_t)height;
	if (channels != 0 && len > max / (size_t)channels) {
		return 0;
	}

	*out = len * (size_t)channels;
	return 1;
}

static unsigned char *read_file(const char *path, size_t *len) {
	FILE *file;
	long size;
	unsigned char *data;

	file = fopen(path, "rb");
	if (file == NULL) {
		fprintf(stderr, "failed to open %s for reading: %s\n", path, strerror(errno));
		return NULL;
	}

	if (fseek(file, 0, SEEK_END) != 0) {
		fprintf(stderr, "failed to seek %s: %s\n", path, strerror(errno));
		fclose(file);
		return NULL;
	}

	size = ftell(file);
	if (size < 0) {
		fprintf(stderr, "failed to size %s: %s\n", path, strerror(errno));
		fclose(file);
		return NULL;
	}

	if (fseek(file, 0, SEEK_SET) != 0) {
		fprintf(stderr, "failed to rewind %s: %s\n", path, strerror(errno));
		fclose(file);
		return NULL;
	}

	data = (unsigned char *)malloc(size > 0 ? (size_t)size : 1);
	if (data == NULL) {
		fprintf(stderr, "failed to allocate %ld bytes for %s\n", size, path);
		fclose(file);
		return NULL;
	}

	if (fread(data, 1, (size_t)size, file) != (size_t)size) {
		fprintf(stderr, "failed to read %s\n", path);
		free(data);
		fclose(file);
		return NULL;
	}

	if (fclose(file) != 0) {
		fprintf(stderr, "failed to close %s: %s\n", path, strerror(errno));
		free(data);
		return NULL;
	}

	*len = (size_t)size;
	return data;
}

static int write_file(const char *path, const void *data, size_t len) {
	FILE *file = fopen(path, "wb");

	if (file == NULL) {
		fprintf(stderr, "failed to open %s for writing: %s\n", path, strerror(errno));
		return 0;
	}

	if (fwrite(data, 1, len, file) != len) {
		fprintf(stderr, "failed to write %s\n", path);
		fclose(file);
		return 0;
	}

	if (fclose(file) != 0) {
		fprintf(stderr, "failed to close %s: %s\n", path, strerror(errno));
		return 0;
	}

	return 1;
}

static int encode_command(int argc, char **argv) {
	unsigned int width;
	unsigned int height;
	unsigned int channels;
	unsigned int colorspace;
	size_t raw_len;
	size_t expected_len;
	unsigned char *raw;
	void *encoded;
	int encoded_len = 0;
	qoi_desc desc;
	int ok;

	if (argc != 8) {
		usage();
		return 2;
	}

	if (
		!parse_uint(argv[2], &width) ||
		!parse_uint(argv[3], &height) ||
		!parse_uint(argv[4], &channels) ||
		!parse_uint(argv[5], &colorspace)
	) {
		fprintf(stderr, "invalid encode arguments\n");
		return 2;
	}

	if ((channels != 3 && channels != 4) || colorspace > 1) {
		fprintf(stderr, "channels must be 3 or 4 and colorspace must be 0 or 1\n");
		return 2;
	}

	if (!checked_raw_len(width, height, channels, &expected_len)) {
		fprintf(stderr, "raw input size overflow\n");
		return 2;
	}

	raw = read_file(argv[6], &raw_len);
	if (raw == NULL) {
		return 1;
	}

	if (raw_len != expected_len) {
		fprintf(stderr, "raw input has %zu bytes, expected %zu\n", raw_len, expected_len);
		free(raw);
		return 1;
	}

	desc.width = width;
	desc.height = height;
	desc.channels = (unsigned char)channels;
	desc.colorspace = (unsigned char)colorspace;

	encoded = qoi_encode(raw, &desc, &encoded_len);
	free(raw);

	if (encoded == NULL || encoded_len <= 0) {
		fprintf(stderr, "qoi_encode failed\n");
		return 1;
	}

	ok = write_file(argv[7], encoded, (size_t)encoded_len);
	QOI_FREE(encoded);

	return ok ? 0 : 1;
}

static int decode_command(int argc, char **argv) {
	unsigned int requested_channels;
	unsigned int output_channels;
	size_t qoi_len;
	size_t raw_len;
	unsigned char *qoi;
	void *raw;
	qoi_desc desc;
	int ok;

	if (argc != 5) {
		usage();
		return 2;
	}

	if (!parse_uint(argv[2], &requested_channels)) {
		fprintf(stderr, "invalid requested channel count\n");
		return 2;
	}

	if (requested_channels != 0 && requested_channels != 3 && requested_channels != 4) {
		fprintf(stderr, "requested channels must be 0, 3, or 4\n");
		return 2;
	}

	qoi = read_file(argv[3], &qoi_len);
	if (qoi == NULL) {
		return 1;
	}

	if (qoi_len > INT_MAX) {
		fprintf(stderr, "qoi input is too large for the C reference API\n");
		free(qoi);
		return 1;
	}

	memset(&desc, 0, sizeof(desc));
	raw = qoi_decode(qoi, (int)qoi_len, &desc, (int)requested_channels);
	free(qoi);

	if (raw == NULL) {
		fprintf(stderr, "qoi_decode failed\n");
		return 1;
	}

	output_channels = requested_channels == 0 ? desc.channels : requested_channels;
	if (!checked_raw_len(desc.width, desc.height, output_channels, &raw_len)) {
		fprintf(stderr, "decoded output size overflow\n");
		QOI_FREE(raw);
		return 1;
	}

	ok = write_file(argv[4], raw, raw_len);
	QOI_FREE(raw);

	return ok ? 0 : 1;
}

int main(int argc, char **argv) {
	if (argc < 2) {
		usage();
		return 2;
	}

	if (strcmp(argv[1], "encode") == 0) {
		return encode_command(argc, argv);
	}

	if (strcmp(argv[1], "decode") == 0) {
		return decode_command(argc, argv);
	}

	usage();
	return 2;
}
