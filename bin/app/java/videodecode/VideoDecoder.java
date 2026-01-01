/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

package videodecode;

import android.content.res.AssetFileDescriptor;
import android.media.MediaCodec;
import android.media.MediaCodec.BufferInfo;
import android.media.MediaExtractor;
import android.media.MediaFormat;
import android.content.Context;
import android.util.Log;
import java.nio.ByteBuffer;

/**
 * Hardware-accelerated video decoder using Android MediaCodec.
 *
 * Decodes video files (H.264/AVC, etc.) and extracts YUV frames for processing.
 * Handles different YUV color formats (planar and semi-planar) across devices.
 *
 * Usage:
 * <pre>
 * VideoDecoder decoder = new VideoDecoder();
 * decoder.setContext(context);
 * decoder.init("video.mp4");
 * int frameCount = decoder.decodeAll();
 * </pre>
 */
public class VideoDecoder {
    private static final boolean DEBUG = false;

    /** YUV420 planar format (YV12/I420) - separate Y, U, V planes */
    private static final int COLOR_FORMATYUV420_PLANAR = 19;

    /** YUV420 semi-planar format (NV21) - Y plane + interleaved UV plane */
    private static final int COLOR_FORMATYUV420_SEMIPLANAR = 21;

    private MediaCodec decoder;
    private MediaExtractor extractor;
    private int width;
    private int height;
    private int decoderId;
    private int outputColorFormat = -1;
    private Context context;

    /** Conditional logging based on DEBUG flag */
    private void log(String fstr, Object... args) {
        if (!DEBUG) return;
        Log.d("darkfi", String.format(fstr, args));
    }

    /** Native callback invoked when a frame is decoded and YUV data is extracted */
    native void onFrameDecoded(int decoderId, byte[] yData, byte[] uData, byte[] vData, int width, int height);

    /** Sets the decoder ID for native callbacks */
    public void setDecoderId(int id) {
        this.decoderId = id;
    }

    /** Sets the Android context for asset loading */
    public void setContext(Context ctx) {
        this.context = ctx;
    }

    /**
     * Initializes the video decoder for a given asset path.
     *
     * Attempts to load from app assets first, falls back to file path.
     * Finds the video track, extracts dimensions, and configures MediaCodec.
     *
     * @param assetPath Path to video file (asset name or absolute path)
     * @return true if initialization succeeded, false otherwise
     */
    public boolean init(String assetPath) {
        try {
            log("init(%s)", assetPath);

            extractor = new MediaExtractor();

            try {
                AssetFileDescriptor afd = context.getAssets().openFd(assetPath);
                extractor.setDataSource(afd.getFileDescriptor(), afd.getStartOffset(), afd.getDeclaredLength());
            } catch (Exception e) {
                extractor.setDataSource(assetPath);
            }

            MediaFormat format = null;
            for (int i = 0; i < extractor.getTrackCount(); i++) {
                format = extractor.getTrackFormat(i);
                String mime = format.getString(MediaFormat.KEY_MIME);
                if (mime != null && mime.startsWith("video/")) {
                    extractor.selectTrack(i);
                    break;
                }
                format = null;
            }

            if (format == null) {
                Log.e("darkfi", "No video track found in: " + assetPath);
                return false;
            }

            width = format.getInteger(MediaFormat.KEY_WIDTH);
            height = format.getInteger(MediaFormat.KEY_HEIGHT);

            decoder = MediaCodec.createDecoderByType(format.getString(MediaFormat.KEY_MIME));
            decoder.configure(format, null, null, 0);

            log("Initialized decoder: %dx%d mime=%s", width, height, format.getString(MediaFormat.KEY_MIME));

            return true;

        } catch (Exception e) {
            Log.e("darkfi", "Failed to initialize video decoder: " + e.getMessage(), e);
            return false;
        }
    }

    /**
     * Decodes all frames from the video.
     *
     * Processes the entire video, extracting YUV data from each frame
     * and invoking the native callback. Handles format changes during decoding.
     *
     * @return Number of frames decoded, or -1 on error
     */
    public int decodeAll() {
        if (decoder == null || extractor == null) {
            Log.e("darkfi", "Decoder not initialized");
            return -1;
        }

        decoder.start();

        MediaFormat outputFormat = decoder.getOutputFormat();
        if (outputFormat != null && outputFormat.containsKey(MediaFormat.KEY_COLOR_FORMAT)) {
            outputColorFormat = outputFormat.getInteger(MediaFormat.KEY_COLOR_FORMAT);
        }
        log("decodeAll() colorFormat=%d", outputColorFormat);

        int frameIndex = 0;
        boolean inputEOS = false;
        boolean outputEOS = false;
        BufferInfo bufferInfo = new BufferInfo();

        try {
            while (!outputEOS) {
                if (!inputEOS) {
                    inputEOS = processInput();
                }

                int result = processOutput(bufferInfo);
                if (result >= 0) {
                    frameIndex += result;
                }

                if ((bufferInfo.flags & MediaCodec.BUFFER_FLAG_END_OF_STREAM) != 0) {
                    outputEOS = true;
                }
            }
        } finally {
            decoder.stop();
            decoder.release();
            extractor.release();
        }

        log("Decoding complete: %d frames", frameIndex);
        return frameIndex;
    }

    /**
     * Feeds compressed video data from the extractor to the decoder.
     *
     * @return true if end of stream was reached, false otherwise
     */
    private boolean processInput() {
        // Try to read some data
        int inputBufferId = decoder.dequeueInputBuffer(10000);
        // Not yet available
        if (inputBufferId < 0)
            return false;

        ByteBuffer inputBuffer = decoder.getInputBuffer(inputBufferId);
        int sampleSize = extractor.readSampleData(inputBuffer, 0);

        // Negative sampleSize means extractor reached end of file
        if (sampleSize < 0) {
            decoder.queueInputBuffer(inputBufferId, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM);
            return true;
        }

        // Success
        decoder.queueInputBuffer(inputBufferId, 0, sampleSize, extractor.getSampleTime(), 0);
        extractor.advance();
        return false;
    }

    /**
     * Retrieves and processes decoded frames from the decoder.
     *
     * @param bufferInfo BufferInfo object to populate with frame metadata
     * @return Number of frames processed (0 or 1), or negative value for info events
     */
    private int processOutput(BufferInfo bufferInfo) {
        int outputBufferId = decoder.dequeueOutputBuffer(bufferInfo, 10000);
        if (outputBufferId >= 0) {
            // New frame to read
            ByteBuffer outputBuffer = decoder.getOutputBuffer(outputBufferId);
            if (outputBuffer != null) {
                processOutputBuffer(outputBuffer, bufferInfo.offset, bufferInfo.size);
            }
            decoder.releaseOutputBuffer(outputBufferId, false);
            return 1;
        } else if (outputBufferId == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
            // Ready to read the output format
            MediaFormat newFormat = decoder.getOutputFormat();
            // We are interested in the color format
            if (newFormat.containsKey(MediaFormat.KEY_COLOR_FORMAT)) {
                outputColorFormat = newFormat.getInteger(MediaFormat.KEY_COLOR_FORMAT);
                log("Format changed: colorFormat=%d", outputColorFormat);
            }
        }
        return 0;
    }

    /**
     * Processes a decoded video frame buffer and extracts YUV data.
     *
     * Handles different color formats:
     * - Semi-planar (NV21): De-interleaves UV data
     * - Planar (YV12/I420): Reads U and V planes directly
     *
     * @param outputBuffer Raw decoded frame data from MediaCodec
     * @param offset Offset to valid data in buffer
     * @param size Size of valid data in bytes
     */
    private void processOutputBuffer(ByteBuffer outputBuffer, int offset, int size) {
        outputBuffer.position(offset);
        outputBuffer.limit(offset + size);

        int ySize = width * height;
        int uvSize = (width / 2) * (height / 2);

        byte[] yData = new byte[ySize];
        byte[] uData = new byte[uvSize];
        byte[] vData = new byte[uvSize];

        outputBuffer.get(yData, 0, ySize);

        if (outputColorFormat == COLOR_FORMATYUV420_PLANAR) {
            outputBuffer.get(uData, 0, uvSize);
            outputBuffer.get(vData, 0, uvSize);
        } else {
            if (outputColorFormat != COLOR_FORMATYUV420_SEMIPLANAR) {
                Log.w("darkfi", String.format("Unknown color format %d, assuming semi-planar", outputColorFormat));
            }
            deinterleaveUV(outputBuffer, uData, vData, uvSize);
        }

        onFrameDecoded(decoderId, yData, uData, vData, width, height);
        outputBuffer.clear();
    }

    /**
     * De-interleaves UV data from semi-planar YUV format (NV21).
     *
     * @param outputBuffer Buffer containing interleaved UVUV... data
     * @param uData Output array for U component
     * @param vData Output array for V component
     * @param uvSize Number of UV pairs to de-interleave
     */
    private void deinterleaveUV(ByteBuffer outputBuffer, byte[] uData, byte[] vData, int uvSize) {
        byte[] uvInterleaved = new byte[uvSize * 2];
        outputBuffer.get(uvInterleaved);
        for (int i = 0; i < uvSize; i++) {
            uData[i] = uvInterleaved[i * 2];
            vData[i] = uvInterleaved[i * 2 + 1];
        }
    }
}
