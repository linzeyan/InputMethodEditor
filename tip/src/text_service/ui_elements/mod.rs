// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 Kan-Ru Chen

mod candidate_list;
mod message_box;
mod notification;

pub(crate) use candidate_list::{CandidateList, FilterKeyResult, Model};
pub(crate) use notification::{Notification, NotificationModel};

use scoped_error::impl_context_error;

impl_context_error!(UiError);
