/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

/// Levels of thread priority
pub enum ThreadPriority {
    Min,
    Low,
    Normal,
    High,
    Max,
}

/// Set current thread priority to given `priority`.
/// This usually requires root privileges.
pub fn set_thread_priority(priority: ThreadPriority) {
    #[cfg(windows)]
    {
        type HANDLE = *mut std::ffi::c_void;

        const THREAD_PRIORITY_IDLE: i32 = -15;
        const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
        const THREAD_PRIORITY_NORMAL: i32 = 0;
        const THREAD_PRIORITY_ABOVE_NORMAL: i32 = 1;
        const THREAD_PRIORITY_TIME_CRITICAL: i32 = 15;

        extern "system" {
            fn GetCurrentThread() -> HANDLE;
            fn SetThreadPriority(hThread: HANDLE, nPriority: i32) -> i32;
        }

        let priority_value = match priority {
            ThreadPriority::Min => THREAD_PRIORITY_IDLE,
            ThreadPriority::Low => THREAD_PRIORITY_BELOW_NORMAL,
            ThreadPriority::Normal => THREAD_PRIORITY_NORMAL,
            ThreadPriority::High => THREAD_PRIORITY_ABOVE_NORMAL,
            ThreadPriority::Max => THREAD_PRIORITY_TIME_CRITICAL,
        };

        unsafe {
            SetThreadPriority(GetCurrentThread(), priority_value);
        }
    }

    #[cfg(target_os = "linux")]
    {
        extern "C" {
            fn setpriority(which: i32, who: u32, prio: i32) -> i32;
        }

        const PRIO_PROCESS: i32 = 0;

        let nice_value = match priority {
            ThreadPriority::Min => 19,
            ThreadPriority::Low => 10,
            ThreadPriority::Normal => 0,
            ThreadPriority::High => -10,
            ThreadPriority::Max => -20,
        };

        unsafe {
            setpriority(PRIO_PROCESS, std::process::id(), nice_value);
        }
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        type mach_port_t = u32;
        type thread_t = mach_port_t;
        type thread_policy_flavor_t = i32;
        type thread_policy_t = *mut i32;
        type mach_msg_type_number_t = u32;

        const THREAD_PRECEDENCE_POLICY: thread_policy_flavor_t = 3;

        #[repr(C)]
        struct thread_precedence_policy_data {
            importance: i32,
        }

        extern "C" {
            fn mach_thread_self() -> thread_t;
            fn thread_policy_set(
                thread: thread_t,
                flavor: thread_policy_flavor_t,
                policy_info: thread_policy_t,
                count: mach_msg_type_number_t,
            ) -> i32;
        }

        let importance = match priority {
            ThreadPriority::Min => -30,
            ThreadPriority::Low => -15,
            ThreadPriority::Normal => 0,
            ThreadPriority::High => 15,
            ThreadPriority::Max => 30,
        };

        unsafe {
            let thread = mach_thread_self();
            let mut policy = thread_precedence_policy_data { importance };
            let count =
                std::mem::size_of::<thread_precedence_policy_data>() as mach_msg_type_number_t / 4;

            thread_policy_set(
                thread,
                THREAD_PRECEDENCE_POLICY,
                &mut policy as *mut _ as thread_policy_t,
                count,
            );
        }
    }
}
