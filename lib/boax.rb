# frozen_string_literal: true

require_relative "boax/version"
require_relative "boax/boax"

module Boax
  class << self
    # Initialize Boax with a project root directory.
    # The directory must contain a node_modules/ folder.
    #
    #   Boax.init(root: __dir__)
    #
    alias_method :_init, :init

    def init(root: ".")
      _init(File.expand_path(root))
    end
  end
end
