//
// INTEL CONFIDENTIAL
//
// Copyright 2013-2014 Intel Corporation All Rights Reserved.
//
// The source code contained or described herein and all documents related
// to the source code ("Material") are owned by Intel Corporation or its
// suppliers or licensors. Title to the Material remains with Intel Corporation
// or its suppliers and licensors. The Material contains trade secrets and
// proprietary and confidential information of Intel or its suppliers and
// licensors. The Material is protected by worldwide copyright and trade secret
// laws and treaty provisions. No part of the Material may be used, copied,
// reproduced, modified, published, uploaded, posted, transmitted, distributed,
// or disclosed in any way without Intel's prior express written permission.
//
// No license under any patent, copyright, trade secret or other intellectual
// property right is granted to or conferred upon you by disclosure or delivery
// of the Materials, either expressly, by implication, inducement, estoppel or
// otherwise. Any license under such intellectual property rights must be
// express and approved by Intel in writing.


var Server = Backbone.Model.extend({
  urlRoot: "/api/host/"
});

var ServerDetail = Backbone.View.extend({
  className: 'server_detail',
  template: _.template($('#server_detail_template').html()),
  render: function () {
    var cleanModel = this.model.toJSON();
    var rendered = this.template({'server': cleanModel});
    $(this.el).find('.ui-dialog-content').html(rendered);
    if (!cleanModel.client_mounts || cleanModel.client_mounts.length < 1) {
      $(this.el).find('tr.client_mounts').hide();
    }

    var generateCommandDropdown = angular.element('html').injector().get('generateCommandDropdown');
    generateCommandDropdown.generateDropdown(
      $(this.el).find('div[command-dropdown]'),
      cleanModel,
      null,
      overrideServerActionClickFactory(serverProfileFactory())
    );
    return this;
  }
});