    pub fn finish_render_into(&mut self, width: u32, height: u32, out: &mut Vec<u8>) {
        let offscreen = self.offscreen.as_mut().unwrap();
        let buf = offscreen.current_buf();
        let buffer_slice = buf.slice(..);
        buffer_slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });

        let data = buffer_slice.get_mapped_range();
        let unpadded_row = (width * 4) as usize;
        let padded_row = offscreen.padded_row_bytes as usize;
        let total = unpadded_row * height as usize;

        out.clear();
        out.reserve(total);
        if padded_row == unpadded_row {
            out.extend_from_slice(&data[..total]);
        } else {
            for row in 0..height as usize {
                let start = row * padded_row;
                out.extend_from_slice(&data[start..start + unpadded_row]);
            }
        }
        drop(data);
        buf.unmap();
        offscreen.flip();

        self.stats.last_readback_bytes = total;
        self.stats.total_readback_bytes += total as u64;
    }

    /// Wait for a previously submitted NV12 render and copy the exact ffmpeg frame into `out`.
    /// Caller must have called `submit_render_nv12` first.
    pub fn finish_render_nv12_into(&mut self, out: &mut Vec<u8>) {
        let nv12 = self.nv12.as_mut().unwrap();
        let frame_size = nv12.frame_size;
        let buf = nv12.current_buf();
        let buffer_slice = buf.slice(..);
        buffer_slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });

        let data = buffer_slice.get_mapped_range();
        out.clear();
        out.reserve(frame_size);
        out.extend_from_slice(&data[..frame_size]);
        drop(data);
        buf.unmap();
        nv12.flip();

        self.stats.last_readback_bytes = frame_size;
        self.stats.total_readback_bytes += frame_size as u64;
    }
