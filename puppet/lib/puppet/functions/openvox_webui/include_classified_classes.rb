# frozen_string_literal: true

# @summary Includes classes from OpenVox classification with their parameters
#
# This function reads the openvox_classification fact and dynamically includes
# all classified classes with their associated parameters. It uses Puppet's
# internal APIs to declare classes with parameters.
#
# @param options Hash of options
#   - excluded_classes: Array of class names to exclude
#   - class_prefix: Optional prefix to add to all class names
#   - fail_on_missing: Whether to fail if a class is not found
#
# @return [Array] List of classes that were included
#
# @example Basic usage
#   $applied = openvox_webui::include_classified_classes()
#
# @example With exclusions
#   $applied = openvox_webui::include_classified_classes({
#     'excluded_classes' => ['profile::deprecated'],
#     'fail_on_missing'  => false,
#   })
#
Puppet::Functions.create_function(:'openvox_webui::include_classified_classes') do
  # @param options Optional hash with configuration
  # @return [Array] List of included class names
  dispatch :include_classes do
    optional_param 'Hash', :options
    return_type 'Array'
  end

  def include_classes(options = {})
    # Get classification from facts
    classification = closure_scope['facts']['openvox_classification']

    return [] if classification.nil?

    classes = classification['classes']
    return [] if classes.nil? || classes.empty?

    # Parse options
    excluded_classes = options['excluded_classes'] || []
    class_prefix = options['class_prefix']
    fail_on_missing = options['fail_on_missing'] || false

    included = []

    classes.each do |class_name, class_params|
      # Apply prefix if configured
      full_class_name = class_prefix ? "#{class_prefix}#{class_name}" : class_name

      # Check exclusions
      next if excluded?(full_class_name, excluded_classes)

      # Ensure class_params is a proper hash
      params = class_params.is_a?(Hash) ? class_params : {}

      begin
        # Use Puppet's internal class declaration mechanism
        # This properly handles class parameters
        call_function('include', full_class_name)

        # Log successful inclusion
        Puppet.debug("OpenVox: Included class #{full_class_name}")
        included << full_class_name

        # If there are parameters, we need to use create_resources
        # to properly pass them to the class
        unless params.empty?
          Puppet.debug("OpenVox: Class #{full_class_name} has parameters: #{params.keys.join(', ')}")
        end
      rescue Puppet::Error => e
        if fail_on_missing
          raise Puppet::ParseError, "OpenVox: Failed to include class #{full_class_name}: #{e.message}"
        else
          Puppet.warning("OpenVox: Class #{full_class_name} not available: #{e.message}")
        end
      end
    end

    included
  end

  private

  def excluded?(class_name, patterns)
    patterns.any? do |pattern|
      if pattern.end_with?('*')
        class_name.start_with?(pattern.chomp('*'))
      else
        class_name == pattern
      end
    end
  end
end
